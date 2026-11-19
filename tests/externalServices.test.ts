import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import { externalServices, configurationToolAvailable } from '../src/externalServices.ts';

test('offline catalogue does not advertise unimplemented services or tools', () => {
  assert.deepEqual(externalServices, []);
  assert.equal(configurationToolAvailable('unknown', 'configure'), false);
  assert.equal(configurationToolAvailable('', ''), false);
});
