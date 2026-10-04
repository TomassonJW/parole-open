#!/usr/bin/env node
// Les liens internes des paquets macOS sont enregistrés sans suivre les liens externes.
import { createHash } from 'node:crypto'
import { createReadStream, promises as fs } from 'node:fs'
import { join, relative, resolve, isAbsolute } from 'node:path'
import { finished } from 'node:stream/promises'

const root = resolve(process.argv[2] ?? '')
if (!process.argv[2]) throw new Error('Indiquez le dossier des paquets')
const files = []
const links = []

async function visit(directory) {
  for (const entry of await fs.readdir(directory, { withFileTypes: true })) {
    const file = join(directory, entry.name)
    if (entry.isSymbolicLink()) {
      const target = await fs.realpath(file)
      const internal = relative(root, target)
      if (internal === '..' || internal.startsWith('../') || isAbsolute(internal))
        throw new Error('Lien symbolique externe dans le paquet')
      links.push({ path: relative(root, file).replaceAll('\\', '/'), target: await fs.readlink(file) })
    } else if (entry.isDirectory()) await visit(file)
    else if (entry.isFile() && file !== join(root, 'SHA256SUMS') && file !== join(root, 'SYMLINKS.json')) files.push(file)
    else if (!entry.isFile()) throw new Error('Type de fichier inattendu dans le paquet')
  }
}

await visit(root)
if (!files.length) throw new Error('Aucun fichier de paquet trouvé')
links.sort((a, b) => a.path.localeCompare(b.path, 'en'))
if (links.length) {
  const path = join(root, 'SYMLINKS.json')
  await fs.writeFile(path, `${JSON.stringify(links, null, 2)}\n`, { flag: 'wx' })
  files.push(path)
}
files.sort((a, b) => a.localeCompare(b, 'en'))
const lines = []
for (const file of files) {
  const name = relative(root, file).replaceAll('\\', '/')
  if (name.includes('\n') || name.includes('\r')) throw new Error('Nom de fichier non pris en charge')
  const hash = createHash('sha256')
  const stream = createReadStream(file)
  stream.on('data', chunk => hash.update(chunk))
  await finished(stream)
  lines.push(`${hash.digest('hex')}  ${name}`)
}
await fs.writeFile(join(root, 'SHA256SUMS'), `${lines.join('\n')}\n`, { flag: 'wx' })
console.log(`Empreintes SHA-256 : ${lines.length} fichiers`)
