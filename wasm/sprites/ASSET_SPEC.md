# Especificación de sprites — RayRunner (Niveles 4 y 5)

Brief para el equipo de arte/sprites. Basado en lo que el motor (`game.html`) espera
realmente y en lo verificado de los PNG actuales.

## Reglas generales (TODAS las hojas)

1. **PNG con transparencia real (RGBA, canal alfa).** Los PNG actuales son RGB sin
   alfa y el juego adivina el fondo por color (chroma-key), lo que come contornos
   oscuros. Con alfa se elimina el problema.
2. Si no hay alfa, usar **un único color de fondo plano** no presente en el arte
   (magenta `#FF00FF`) en TODA la hoja, uniforme.
3. **Rejilla de celdas uniformes**: ancho_celda = PNG_ancho / columnas,
   alto_celda = PNG_alto / filas. Todas iguales.
4. **Cada sprite centrado en su celda**, con ≥8% de margen y un **hueco
   transparente de ≥6 px entre sprites** (evita que se cuele "un trozo del vecino").
5. **Ningún sprite se sale de su celda.**
6. **Pies del personaje pegados a la parte baja de la celda** (el juego ancla por
   los pies).
7. Entregar el **JSON acompañante con coordenadas que coincidan de verdad** con el
   PNG (mismo tamaño, mismo `meta.image`, coords dentro de límites).

---

## 1) `hero.png` — héroe mazmorra (Nivel 4)

- **Rejilla: 7 columnas × 6 filas**, celdas uniformes. Recomendado **1344×1152**
  (celda 192×192).
- **Mira a la DERECHA** (el juego voltea para ir a la izquierda).
- Filas (arriba→abajo), rellenar las 7 columnas con los fotogramas:
  1. Fila 0 → **idle**
  2. Fila 1 → **andar/correr**
  3. Fila 2 → **ataque** (barrido de arma completo)
  4. Fila 3 → **salto** (ascenso) y **caída**
  5. Fila 4 → **daño** (2-3 frames) + **muerte**
  6. Fila 5 → opcional (ataque pesado/victoria)
- Escala del cuerpo constante entre frames.

## 2) `enemies.png` — enemigos mazmorra (Nivel 4)

- **Rejilla: 6 columnas × 6 filas**, celdas uniformes (recomendado 1152×1152,
  celda 192×192).
- **Miran a la IZQUIERDA** (el juego voltea para la derecha).
- **Una criatura por fila**, en este orden exacto:
  1. Fila 0 → goblin
  2. Fila 1 → esqueleto
  3. Fila 2 → orco
  4. Fila 3 → slime
  5. Fila 4 → murciélago
  6. Fila 5 → caballero oscuro
- **Columnas (estados) en este orden exacto**:
  `idle · andar1 · andar2 · ataque · golpe · muerte`.

## 3) `extras.png` — fondos + tiles mazmorra (Nivel 4)  ⚠️ el más problemático

El JSON actual describe un tileset por índices que **NO coincide** con el arte real
(por eso el "suelo" salía bosque y aparecían props raros). Dos entregables:

**A. Fondos de bioma** — 4 escenas completas, cada una un rectángulo limpio y
**dentro de los límites** del PNG:
`castle_night`, `castle_red_sky`, `forest`, `cave` (o los 4 biomas elegidos).
Cada escena un bloque contiguo (sin mezclar dos escenas en un mismo recorte).

**B. Props/tiles** con **coordenadas exactas verificadas** (no índices nominales):
`dungeon_floor`, `dungeon_column`, `dungeon_arch`, `chains`, `torch_fire`,
`dungeon_door`, `castle_bridge` (plataformas).

**Formato JSON obligatorio (rects con coords reales):**
```json
{
  "meta": { "image": "extras.png", "imageW": 1536, "imageH": 1024 },
  "backgrounds": {
    "castle_red_sky": { "x": 1120, "y": 384, "w": 400, "h": 320 }
  },
  "tiles": {
    "dungeon_floor":  { "x": 0, "y": 0, "w": 64,  "h": 64  },
    "dungeon_column": { "x": 0, "y": 0, "w": 64,  "h": 192 },
    "chains":         { "x": 0, "y": 0, "w": 32,  "h": 128 },
    "torch_fire":     { "x": 0, "y": 0, "w": 48,  "h": 64  },
    "castle_bridge":  { "x": 0, "y": 0, "w": 128, "h": 32  }
  }
}
```
Cada `x/y/w/h` debe caer **dentro** de `imageW×imageH` y apuntar **al arte real**
(verificarlo abriendo el PNG). Este ha sido el fallo: coordenadas fuera de rango o
apuntando a otra cosa.

## 4) `comando_weapons.png` — armas (Nivel 5)

- **Rejilla: 6 columnas × 4 filas**, celdas uniformes (recomendado 768×512,
  celda 128×128, o mayor manteniendo 6×4).
- JSON puede ser array (`frames:[{id,name,x,y,w,h}]`), pero el **PNG debe ser
  exactamente 6×4** para que el troceado por rejilla cuadre.
- **Nombres y orden que el juego usa** (por fila):
  1. Fila 0 (bazooka): `bazooka_aim_01, bazooka_aim_02, bazooka_fire, bazooka_recoil, bazooka_end, rocket`
  2. Fila 1 (lanzamisiles): `missile_aim_01, missile_aim_02, missile_fire, missile_recoil, missile_end, missile`
  3. Fila 2 (dobles): `dualgun_idle, dualgun_ready, dualgun_fire_01, dualgun_fire_02, dualgun_fire_03, bullets`
  4. Fila 3 (uzi): `uzi_idle, uzi_ready, uzi_fire_01, uzi_fire_02, uzi_fire_03, bullets_fast`
- Las armas **apuntan hacia ARRIBA** (top-down); el juego rota según el rumbo.

## 5) `enemy_units.png`, `backgrounds.png`, `world_tiles.png` (Nivel 5)

- **Ya están correctos** (1536×1024 y coinciden con sus JSON). No hace falta
  regenerarlos.
- Si se regeneran, mantener: `enemy_units` con variantes `soldier/tank/jeep/moto`
  × `desert/green/black/red|camo`; `backgrounds` con los 24 tiles actuales;
  `world_tiles` con los grupos actuales (palms, houses, vegetation, roads, etc.).

---

## Checklist de entrega

1. PNG en RGBA con transparencia real.
2. Rejilla uniforme + hueco transparente entre sprites.
3. JSON con `meta.image` correcto y coordenadas **verificadas dentro de límites**.
4. Dirección de mirada consistente por hoja (héroe→derecha, enemigos→izquierda,
   armas→arriba).
5. Confirmar dimensiones exactas del PNG y filas×columnas.
