import { strict as assert } from 'node:assert';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { checkReleaseVersion } from './check-release-version.mjs';

function fixture(t, version = '0.1.0', overrides = {}) {
  const root = mkdtempSync(join(tmpdir(), 'chatwise-release-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  mkdirSync(join(root, 'src-tauri'));
  const files = {
    'package.json': JSON.stringify({ name: 'oh-my-chatwise', version }),
    'src-tauri/tauri.conf.json': JSON.stringify({ version }),
    'src-tauri/Cargo.toml': `[package]\nname = "oh-my-chatwise"\nversion = "${version}"\n\n[dependencies]\nserde = "1"\n`,
    'src-tauri/Cargo.lock': `version = 4\n\n[[package]]\nname = "another-crate"\nversion = "9.0.0"\n\n[[package]]\nname = "oh-my-chatwise"\nversion = "${version}"\n`,
    ...overrides,
  };
  for (const [file, content] of Object.entries(files)) writeFileSync(join(root, file), content);
  return root;
}

test('branch builds validate all manifests without requiring a tag', (t) => {
  assert.deepEqual(checkReleaseVersion(fixture(t)), { version: '0.1.0', prerelease: false });
});

test('stable and prerelease tags are classified correctly', (t) => {
  assert.equal(checkReleaseVersion(fixture(t), 'v0.1.0').prerelease, false);
  assert.deepEqual(checkReleaseVersion(fixture(t, '1.2.3-beta.1'), 'v1.2.3-beta.1'), {
    version: '1.2.3-beta.1',
    prerelease: true,
  });
});

test('rejects wrong tags and shell/expression-like tag input', (t) => {
  const root = fixture(t);
  for (const tag of ['v0.2.0', '0.1.0', 'v0.1.0\nversion=evil', 'v$(touch unwanted)'])
    assert.throws(() => checkReleaseVersion(root, tag), /does not match/);
});

test('rejects a stale Tauri version, Cargo manifest or Cargo lockfile', (t) => {
  for (const file of [
    'src-tauri/tauri.conf.json',
    'src-tauri/Cargo.toml',
    'src-tauri/Cargo.lock',
  ]) {
    const root = fixture(t);
    const content = readFileSync(join(root, file), 'utf8').replaceAll('0.1.0', '0.2.0');
    writeFileSync(join(root, file), content);
    assert.throws(() => checkReleaseVersion(root, 'v0.1.0'), /does not match/);
  }
});

test('rejects malformed or ambiguous versions before writing workflow outputs', (t) => {
  for (const version of ['01.2.3', '1.2', '1.2.3-01', '1.2.3+build.1', '1.2.3\n'])
    assert.throws(() => checkReleaseVersion(fixture(t, version)));
});
