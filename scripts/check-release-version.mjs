import { appendFileSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

const numeric = '(?:0|[1-9]\\d*)';
const identifier = '(?:0|[1-9]\\d*|\\d*[A-Za-z-][0-9A-Za-z-]*)';
const versionPattern = new RegExp(
  `^${numeric}\\.${numeric}\\.${numeric}(?:-${identifier}(?:\\.${identifier})*)?$`,
);

function tomlString(section, key) {
  const match = section?.match(new RegExp(`^${key}\\s*=\\s*"([^"\\r\\n]+)"\\s*$`, 'm'));
  if (!match) throw new Error(`Cannot read TOML field: ${key}`);
  return match[1];
}

export function checkReleaseVersion(root, tag = '') {
  const read = (file) => readFileSync(resolve(root, file), 'utf8');
  const pkg = JSON.parse(read('package.json'));
  const tauri = JSON.parse(read('src-tauri/tauri.conf.json'));
  const cargo = read('src-tauri/Cargo.toml')
    .split(/^\[package\]\s*$/m)[1]
    ?.split(/^\[/m)[0];
  const locked = read('src-tauri/Cargo.lock')
    .split(/^\[\[package\]\]\s*$/m)
    .slice(1)
    .find((section) => tomlString(section, 'name') === pkg.name);
  const versions = {
    'package.json': pkg.version,
    'src-tauri/tauri.conf.json': tauri.version,
    'src-tauri/Cargo.toml': tomlString(cargo, 'version'),
    'src-tauri/Cargo.lock': tomlString(locked, 'version'),
  };
  const version = pkg.version;
  if (typeof version !== 'string' || !versionPattern.test(version))
    throw new Error(
      'Use a version such as 1.2.3 or 1.2.3-beta.1 (no leading zeroes/build metadata).',
    );
  for (const [file, actual] of Object.entries(versions)) {
    if (actual !== version) throw new Error(`${file} version ${actual} does not match ${version}.`);
  }
  if (tag && tag !== `v${version}`)
    throw new Error(`Release tag ${tag} does not match application version v${version}.`);
  return { version, prerelease: version.includes('-') };
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  try {
    const { version, prerelease } = checkReleaseVersion(
      process.cwd(),
      process.argv[2] ?? process.env.RELEASE_TAG ?? '',
    );
    if (process.env.GITHUB_OUTPUT)
      appendFileSync(process.env.GITHUB_OUTPUT, `version=${version}\nprerelease=${prerelease}\n`);
    console.log(`Verified application version ${version} (prerelease: ${prerelease}).`);
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
