// Ranking global de RayRunner — Vercel Serverless Function (Node.js, sin
// dependencias: solo `fetch`, ya global en el runtime Node 18+ de Vercel).
//
// Almacen: Upstash Redis, via su API REST — un sorted set (`raydrone:scores`)
// es la estructura de datos hecha justo para esto: ZADD para guardar una
// puntuacion, ZREVRANGE para traer el top ordenado en un unico comando.
// No hace falta SQL, ni un ORM, ni un servidor que mantener.
//
// Variables de entorno esperadas (las inyecta Vercel al conectar la
// integracion de Upstash desde el dashboard, pestana Storage). Se aceptan
// los dos nombres que ha usado Vercel para esto, por si acaso:
//   KV_REST_API_URL / KV_REST_API_TOKEN              (integracion "Vercel KV")
//   UPSTASH_REDIS_REST_URL / UPSTASH_REDIS_REST_TOKEN (Upstash directo)

const REST_URL = process.env.KV_REST_API_URL || process.env.UPSTASH_REDIS_REST_URL;
const REST_TOKEN = process.env.KV_REST_API_TOKEN || process.env.UPSTASH_REDIS_REST_TOKEN;
const KEY = 'raydrone:scores';
const MAX_KEEP = 200; // cuantas entradas se conservan en el almacen
const TOP_N = 10;     // cuantas se devuelven al pedir el ranking

// Records "leyenda": marcas base que siempre aparecen en el ranking (se
// fusionan con las reales de Upstash, ordenadas y sin duplicar). Mismo set
// que el cliente para que local y global coincidan.
const SEED = [
    { n: 'BRUZOS', s: 13792, l: 1, d: 1 },
    { n: 'THOR', s: 10230, l: 1, d: 2 },
];
function mergeSeed(list) {
    const merged = [...list, ...SEED].sort((a, b) => b.s - a.s);
    const seen = new Set(), out = [];
    for (const e of merged) { const k = `${e.n}|${e.s}`; if (!seen.has(k)) { seen.add(k); out.push(e); } }
    return out;
}

// Ejecuta una tanda de comandos Redis via el endpoint /pipeline de Upstash
// (varios comandos en un unico viaje de red) y devuelve sus resultados en orden.
async function redisPipeline(commands) {
    const res = await fetch(`${REST_URL}/pipeline`, {
        method: 'POST',
        headers: { Authorization: `Bearer ${REST_TOKEN}`, 'Content-Type': 'application/json' },
        body: JSON.stringify(commands),
    });
    if (!res.ok) throw new Error(`Upstash respondio ${res.status}`);
    const data = await res.json();
    return data.map((r) => r.result);
}

// El nick lo escribe el usuario: nunca confiar en su forma. Se quitan los
// caracteres de control (el cliente solo hace trim+slice; esto es un
// cinturon extra por si la peticion no viniera de ese formulario) y se
// recorta a 12 - la misma cota que ya aplica el campo del cliente.
function sanitizeNick(n) {
    if (typeof n !== 'string') return 'anon';
    let clean = '';
    for (const ch of n) {
        const c = ch.codePointAt(0);
        if (c >= 0x20 && c !== 0x7f) clean += ch;
    }
    clean = clean.trim().slice(0, 12);
    return clean || 'anon';
}

module.exports = async (req, res) => {
    res.setHeader('Access-Control-Allow-Origin', '*');
    res.setHeader('Access-Control-Allow-Methods', 'GET, POST, OPTIONS');
    res.setHeader('Access-Control-Allow-Headers', 'Content-Type');
    if (req.method === 'OPTIONS') { res.status(204).end(); return; }

    if (!REST_URL || !REST_TOKEN) {
        // Sin la integracion de Upstash conectada todavia en este proyecto de
        // Vercel: el cliente hace fallback a su ranking local (ver game.html).
        res.status(503).json({ error: 'Ranking global no configurado (falta Upstash).' });
        return;
    }

    try {
        if (req.method === 'GET') {
            // Se piden más de las que se devuelven porque al fusionar los records
            // "leyenda" y deduplicar podría recortarse el top real.
            const [entries] = await redisPipeline([['ZREVRANGE', KEY, 0, TOP_N + SEED.length]]);
            const real = (entries || []).map((m) => { try { return JSON.parse(m); } catch (_) { return null; } }).filter(Boolean);
            const top = mergeSeed(real).slice(0, TOP_N);
            res.status(200).json({ top });
            return;
        }

        if (req.method === 'POST') {
            const body = typeof req.body === 'string' ? JSON.parse(req.body || '{}') : (req.body || {});
            const score = Math.max(0, Math.min(999999, Math.floor(Number(body.score))));
            const level = (body.level === 2 || body.level === 3) ? body.level : 1;
            if (!Number.isFinite(score)) { res.status(400).json({ error: 'score invalido' }); return; }
            const nick = sanitizeNick(body.nick);
            const entry = { n: nick, s: score, l: level, d: Date.now() };
            const member = JSON.stringify(entry);
            const [, , rank, total] = await redisPipeline([
                ['ZADD', KEY, score, member],
                ['ZREMRANGEBYRANK', KEY, 0, -(MAX_KEEP + 1)], // conserva solo las MAX_KEEP mejores
                ['ZREVRANK', KEY, member],
                ['ZCARD', KEY],
            ]);
            res.status(200).json({ pos: (rank ?? 0) + 1, total: total ?? 1 });
            return;
        }

        res.status(405).json({ error: 'metodo no soportado' });
    } catch (err) {
        res.status(502).json({ error: 'Upstash no disponible', detail: String(err) });
    }
};
