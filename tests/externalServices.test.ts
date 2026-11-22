import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import { externalServices, enabledConnectorServices, configurationToolAvailable, testConnector, toggleConnector, type ExternalService } from '../src/externalServices.ts';

test('catalogue does not advertise unimplemented tools', async () => {
  assert.ok(externalServices.length >= 2);
  for (const service of externalServices) {
    assert.ok(service.method && service.requirements && service.permissions && service.readTools && service.writeTools && service.limitations);
    assert.equal(await testConnector(service), 'not_implemented');
  }
  assert.equal(configurationToolAvailable('unknown', 'configure'), false);
});
test('several selections and fake connector diagnostics remain independent', async () => {
  const ids = toggleConnector(toggleConnector([], 'one', true), 'two', true);
  assert.deepEqual(ids, ['one', 'two']);
  assert.deepEqual(toggleConnector(ids, 'one', false), ['two']);
  const fake: ExternalService = { id: 'fake', service: 'Fake', method: 'API', requirements: 'none', permissions: 'read', readTools: 'read', writeTools: 'none', limitations: 'test', implemented: true, tested: true };
  assert.equal(await testConnector(fake, async () => true), 'available');
  assert.equal(await testConnector(fake, async () => { throw Error('offline'); }), 'failed');
  assert.equal(await testConnector({ ...fake, tested: false }, async () => true), 'untested');
});

test('fake catalogue drives selection, availability and individual fake connector diagnostics', async () => {
  const fake: ExternalService = { id: 'alpha', service: 'Alpha', method: 'API', requirements: 'none', permissions: 'read', readTools: 'read', writeTools: 'update', limitations: 'fixture', implemented: true, tested: true };
  const catalogue: ExternalService[] = [fake, { ...fake, id: 'beta', implemented: false }, { ...fake, id: 'gamma', tested: false }];
  const selected = toggleConnector(toggleConnector([], 'alpha', true), 'beta', true);
  const enabled = enabledConnectorServices(catalogue, [...selected, 'unknown']);
  assert.deepEqual(enabled.map(service => service.id), ['alpha', 'beta']);
  assert.equal(configurationToolAvailable('alpha', 'update', catalogue), true);
  assert.equal(configurationToolAvailable('beta', 'update', catalogue), false);
  assert.equal(configurationToolAvailable('gamma', 'update', catalogue), false);
  assert.equal(configurationToolAvailable('unknown', 'update', catalogue), false);
  let calls = 0;
  const probe = async () => { calls++; return true; };
  assert.equal(await testConnector(enabled[0], probe), 'available');
  assert.equal(await testConnector(enabled[1], probe), 'not_implemented');
  assert.equal(await testConnector(catalogue[2], probe), 'untested');
  assert.equal(calls, 1);
  assert.deepEqual(enabledConnectorServices(catalogue, toggleConnector(selected, 'alpha', false)).map(service => service.id), ['beta']);
});
