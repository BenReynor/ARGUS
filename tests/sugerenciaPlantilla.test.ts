import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { createInterface } from 'node:readline';
import { extraerSugerencia, SugerenciaPendiente } from '../src/sugerenciaPlantilla.ts';

test('agente falso consulta la biblioteca y el usuario revisa, descarta o acepta una copia', async () => {
  const original = { id: 'original', name: 'Original', roles: ['Reader'] };
  const biblioteca = [structuredClone(original)];
  const llamadas: string[] = [];
  // El proceso actúa como agente: solicita herramientas y utiliza sus respuestas.
  const agente = spawn(process.execPath, ['-e', `
    const rl = require('node:readline').createInterface({input: process.stdin});
    let etapa = 0;
    console.log(JSON.stringify({tool: 'listar_plantillas'}));
    rl.on('line', line => {
      const respuesta = JSON.parse(line);
      if (etapa++ === 0) console.log(JSON.stringify({tool: 'consultar_plantilla', id: respuesta[0].id}));
      else {
        const copia = {...respuesta, id: 'copia', roles: [...respuesta.roles, 'Writer']};
        console.log(JSON.stringify({text: 'Plan basado en ' + respuesta.name + '.\\n' + '\x60\x60\x60argus-template\\n' + JSON.stringify(copia) + '\\n\x60\x60\x60'}));
        rl.close();
      }
    });
  `], { stdio: ['pipe', 'pipe', 'pipe'] });
  let texto = '';
  for await (const linea of createInterface({ input: agente.stdout })) {
    const mensaje = JSON.parse(linea);
    if (mensaje.tool === 'listar_plantillas') {
      llamadas.push(mensaje.tool);
      agente.stdin.write(JSON.stringify(biblioteca.map(({ id, name }) => ({ id, name }))) + '\n');
    } else if (mensaje.tool === 'consultar_plantilla') {
      llamadas.push(mensaje.tool);
      agente.stdin.write(JSON.stringify(biblioteca.find(item => item.id === mensaje.id)) + '\n');
    } else { texto = mensaje.text; agente.stdin.end(); }
  }
  assert.deepEqual(llamadas, ['listar_plantillas', 'consultar_plantilla']);
  assert.match(texto, /Plan basado en Original/);
  const data = extraerSugerencia(texto)!;
  const guardar = async (documento: string) => { biblioteca.push(JSON.parse(documento)); };
  const revisar = async (documento: string) => { assert.equal(JSON.parse(documento).name, original.name); };

  const descartada = new SugerenciaPendiente(data);
  await descartada.revisar(revisar);
  descartada.descartar();
  await assert.rejects(descartada.aceptar(guardar));
  assert.deepEqual(biblioteca, [original]);

  const aceptada = new SugerenciaPendiente(data);
  await assert.rejects(aceptada.aceptar(guardar));
  await aceptada.revisar(revisar);
  assert.deepEqual(biblioteca, [original]);
  await aceptada.aceptar(guardar);
  assert.deepEqual(biblioteca, [original, { id: 'copia', name: 'Original', roles: ['Reader', 'Writer'] }]);
  await assert.rejects(aceptada.aceptar(guardar));
});
