//! Durée de vie d'un enfant natif détenu exclusivement par le backend.
//!
//! Cette primitive ne constitue pas un superviseur complet : pas de limite
//! mémoire/CPU, d'arrêt d'un arbre, de protection après mort brutale du parent,
//! de reprise automatique, de validation des messages ou de résultat métier.
//! `wait` est bloquant et doit tourner hors du fil de l'interface. L'alternative
//! `poll` exige un appel régulier ; un objet dormant ne surveille pas son délai.
use crate::child_process::suppress_child_console;
use std::{
    io,
    path::Path,
    process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, ExitStatus},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StopReason {
    Cancelled,
    Deadline,
}

/// Ce signal ne peut pas être transféré à un autre lancement : chaque enfant
/// reçoit une cellule indépendante. Il ne contient aucun PID fourni de dehors.
#[derive(Clone)]
pub struct Cancellation(Arc<AtomicBool>);
impl Cancellation {
    /// Demande idempotente, appliquée au prochain `poll` ou par `wait`.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ProcessOutcome {
    pub pid: u32,
    pub status: ExitStatus,
    /// Une requête d'arrêt a été faite, pas une preuve de sa causalité :
    /// une sortie naturelle peut coïncider avec la requête OS.
    pub stop_requested: Option<StopReason>,
}

/// Flux configurés par le backend, transférés une seule fois au transport borné.
pub struct ProcessPipes {
    pub stdin: Option<ChildStdin>,
    pub stdout: Option<ChildStdout>,
    pub stderr: Option<ChildStderr>,
}

/// Un seul propriétaire du handle natif. Les erreurs de lancement ne produisent
/// ni objet en cours ni PID fictif. Ne pas exposer la Command à une fenêtre.
pub struct ManagedProcess {
    child: Child,
    pid: u32,
    outcome: Option<ProcessOutcome>,
    cancellation: Cancellation,
    deadline: Instant,
}
impl ManagedProcess {
    /// Exécutable absolu, arguments et variables choisis par le backend. Seules
    /// les variables explicitement définies sur Command sont transmises. Aucun
    /// PATH ambiant, jeton, clé ou configuration modèle n'est copié implicitement.
    /// Les flux suivent la configuration explicite de Command ; cette primitive
    /// ne lit ni n'accumule leur contenu. Le futur service doit les borner.
    pub fn spawn(mut command: Command, budget: Duration) -> io::Result<Self> {
        if budget.is_zero() || !Path::new(command.get_program()).is_absolute() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Lancement local ou délai invalide",
            ));
        }
        let deadline = Instant::now().checked_add(budget).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "Délai de calcul invalide")
        })?;
        let environment: Vec<_> = command
            .get_envs()
            .filter_map(|(key, value)| {
                value.map(|value| (key.to_os_string(), value.to_os_string()))
            })
            .collect();
        command.env_clear().envs(environment);
        suppress_child_console(&mut command);
        let child = command.spawn()?;
        let pid = child.id();
        Ok(Self {
            child,
            pid,
            outcome: None,
            cancellation: Cancellation(Arc::new(AtomicBool::new(false))),
            deadline,
        })
    }
    /// Le propriétaire reste chargé de l'arrêt et de la récolte. La remise des
    /// flux ne valide pas les messages et ne démarre pas de lecteurs cachés.
    pub fn take_pipes(&mut self) -> ProcessPipes {
        ProcessPipes {
            stdin: self.child.stdin.take(),
            stdout: self.child.stdout.take(),
            stderr: self.child.stderr.take(),
        }
    }
    pub fn pid(&self) -> u32 {
        self.pid
    }
    pub fn cancellation(&self) -> Cancellation {
        self.cancellation.clone()
    }

    fn finish(&mut self, status: ExitStatus, reason: Option<StopReason>) -> ProcessOutcome {
        let outcome = ProcessOutcome {
            pid: self.pid,
            status,
            stop_requested: reason,
        };
        self.outcome = Some(outcome);
        outcome
    }

    /// Une sortie observée avant la requête d'arrêt reste une sortie normale.
    /// Les résultats terminaux sont conservés : aucun signal tardif ne les réécrit.
    /// L'arrêt appelle kill puis wait sur le Child, jamais sur un PID libre.
    pub fn poll(&mut self) -> io::Result<Option<ProcessOutcome>> {
        if let Some(outcome) = self.outcome {
            return Ok(Some(outcome));
        }
        if let Some(status) = self.child.try_wait()? {
            return Ok(Some(self.finish(status, None)));
        }
        let reason = if self.cancellation.0.load(Ordering::Acquire) {
            Some(StopReason::Cancelled)
        } else if Instant::now() >= self.deadline {
            Some(StopReason::Deadline)
        } else {
            None
        };
        if let Some(reason) = reason {
            if let Err(error) = self.child.kill() {
                // L'enfant peut avoir terminé entre try_wait et kill. Une autre
                // erreur ne devient jamais une annulation réussie inventée.
                if let Some(status) = self.child.try_wait()? {
                    return Ok(Some(self.finish(status, None)));
                }
                return Err(error);
            }
            let status = self.child.wait()?;
            return Ok(Some(self.finish(status, Some(reason))));
        }
        Ok(None)
    }

    /// L'OS peut retarder la récolte d'un enfant. Le délai déclenche une demande
    /// d'arrêt, ce n'est pas une garantie de terminaison à la milliseconde près.
    pub fn wait(&mut self) -> io::Result<ProcessOutcome> {
        loop {
            if let Some(outcome) = self.poll()? {
                return Ok(outcome);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
impl Drop for ManagedProcess {
    fn drop(&mut self) {
        if self.outcome.is_none() && !matches!(self.child.try_wait(), Ok(Some(_))) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}
