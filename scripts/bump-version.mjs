// Écrit une version semver dans tous les fichiers qui la portent — seule liste de ces fichiers,
// utilisée par le workflow release.yml et utilisable en local :
//   node scripts/bump-version.mjs 1.2.3
// Les sous-crates internes (chelou-manifest, chelou-providers, keyring-dpapi-store) ne sont pas
// publiés et gardent leur propre version.
import { readFileSync, writeFileSync } from 'node:fs';

const version = process.argv[2];
if (!/^\d+\.\d+\.\d+$/.test(version ?? '')) {
  console.error('Usage : node scripts/bump-version.mjs X.Y.Z');
  process.exit(1);
}

/** Remplace l'unique match de `pattern` (groupe 1 = préfixe gardé tel quel) et échoue sinon. */
function replaceOnce(path, pattern, check) {
  const before = readFileSync(path, 'utf8');
  const matches = before.match(new RegExp(pattern.source, `${pattern.flags}g`));
  if (matches?.length !== 1) {
    throw new Error(`${path} : ${matches?.length ?? 0} occurrence(s) de ${pattern}, 1 attendue`);
  }
  const after = before.replace(pattern, `$1${version}"`);
  writeFileSync(path, after);
  if (check(after) !== version) throw new Error(`${path} : version non mise à jour`);
}

const jsonVersion = (text) => JSON.parse(text).version;

// Le "version" de premier niveau est le seul indenté de 2 espaces dans ces deux fichiers.
replaceOnce('package.json', /^( {2}"version": ")[^"]*"/m, jsonVersion);
replaceOnce('src-tauri/tauri.conf.json', /^( {2}"version": ")[^"]*"/m, jsonVersion);

// package-lock.json : écrit par npm en JSON.stringify(_, null, 2) — on fait pareil.
const lockPath = 'package-lock.json';
const lock = JSON.parse(readFileSync(lockPath, 'utf8'));
lock.version = version;
lock.packages[''].version = version;
writeFileSync(lockPath, `${JSON.stringify(lock, null, 2)}\n`);

// Cargo.toml : seule la section [package] du crate principal (pas les dépendances).
replaceOnce(
  'src-tauri/Cargo.toml',
  /(\[package\]\r?\nname = "chelou-track"\r?\nversion = ")[^"]*"/,
  (text) => text.match(/\[package\]\r?\nname = "chelou-track"\r?\nversion = "([^"]*)"/)?.[1],
);
replaceOnce(
  'src-tauri/Cargo.lock',
  /(name = "chelou-track"\r?\nversion = ")[^"]*"/,
  (text) => text.match(/name = "chelou-track"\r?\nversion = "([^"]*)"/)?.[1],
);

console.log(`Version ${version} écrite.`);
