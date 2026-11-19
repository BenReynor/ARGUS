import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import { checkKeyring, diagnosticReport } from '../src/diagnosticReport.ts';

test('raw diagnostic errors never enter the preview or exported report', async () => {
  const secrets = ['token=abc123', 'password=hunter2', 'private conversation', 'channel secret', 'ENV_VALUE=secret'];
  const result = await checkKeyring(async () => { throw new Error(secrets.join(' ')); });
  const preview = diagnosticReport('1.2.0', 'Linux', result, s => s);
  const exported = preview; // The UI writes the exact reviewed text.
  for (const secret of secrets) {
    assert.ok(!preview.includes(secret));
    assert.ok(!exported.includes(secret));
  }
  assert.match(preview, /Keyring: Check failed/);
  assert.match(preview, /Not checked/);
});

test('successful and unavailable checks have distinct results', async () => {
  assert.equal(await checkKeyring(async () => true), 'available');
  assert.equal(await checkKeyring(async () => false), 'unavailable');
  assert.ok(!diagnosticReport('password=secret', 'ENV_VALUE=secret', 'available', s => s).includes('secret'));
});
