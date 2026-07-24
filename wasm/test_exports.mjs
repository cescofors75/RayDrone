// Contrato JS ↔ wasm: comprueba que raydrone.wasm exporta TODO lo que el
// AudioWorklet le pide.
//
// El .wasm no está versionado (lo genera build.sh), así que es muy fácil tirar
// de git, arrancar la página y encontrarse con un
// "TypeError: this.ex.<algo> is not a function" en el navegador. Esa clase de
// fallo se detecta aquí, con el binario recién compilado, en vez de en runtime.

import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const processorSrc = readFileSync(join(here, 'processor.js'), 'utf8');
const bytes = readFileSync(join(here, 'raydrone.wasm'));

// Todo lo que el worklet toca del módulo se escribe como `ex.foo` o `this.ex.foo`.
const used = new Set();
for (const m of processorSrc.matchAll(/\bex\.([A-Za-z_][A-Za-z0-9_]*)/g)) used.add(m[1]);

// Lista explícita que el constructor da por sentada (debe seguir a processor.js).
const requiredMatch = processorSrc.match(/const REQUIRED_EXPORTS = \[([\s\S]*?)\];/);
const required = requiredMatch
    ? [...requiredMatch[1].matchAll(/'([^']+)'/g)].map((m) => m[1])
    : [];

const exports = new WebAssembly.Instance(new WebAssembly.Module(bytes), {}).exports;
const have = new Set(Object.keys(exports));

let failed = 0;
const check = (label, ok, detail) => {
    console.log(`  ${ok ? '✓' : '✗'}  ${label}${detail ? `  — ${detail}` : ''}`);
    if (!ok) failed++;
};

console.log('RayDrone — contrato de exports JS ↔ wasm\n');

const missingUsed = [...used].filter((n) => !have.has(n));
check(
    `el .wasm exporta los ${used.size} símbolos que usa processor.js`,
    missingUsed.length === 0,
    missingUsed.length ? `faltan: ${missingUsed.join(', ')}` : `todos presentes`,
);

const missingRequired = required.filter((n) => !have.has(n));
check(
    `están los ${required.length} exports que el constructor exige`,
    required.length > 0 && missingRequired.length === 0,
    missingRequired.length ? `faltan: ${missingRequired.join(', ')}` : 'REQUIRED_EXPORTS completo',
);

// REQUIRED_EXPORTS solo sirve para dar un mensaje claro: si se queda corto
// respecto a lo que el constructor llama de verdad, vuelve el TypeError crudo.
const ctorBody = processorSrc.slice(
    processorSrc.indexOf('constructor(options)'),
    processorSrc.indexOf('    onMsg(d)'),
);
const ctorUses = new Set();
for (const m of ctorBody.matchAll(/\bex\.([A-Za-z_][A-Za-z0-9_]*)/g)) ctorUses.add(m[1]);
const unguarded = [...ctorUses].filter((n) => !required.includes(n));
check(
    'REQUIRED_EXPORTS cubre lo que el constructor invoca',
    unguarded.length === 0,
    unguarded.length ? `sin cubrir: ${unguarded.join(', ')}` : 'sin huecos',
);

check('la memoria del módulo es accesible', exports.memory instanceof WebAssembly.Memory);

console.log(failed ? `\n❌ ${failed} FALLO(S)` : '\n✅ ALL PASSED');
process.exit(failed ? 1 : 0);
