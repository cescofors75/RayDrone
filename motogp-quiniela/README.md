# Quiniela MotoGP 2026-27

Juego de predicciones (quiniela) para la temporada de MotoGP, 100% HTML/CSS/JS,
sin backend ni dependencias — todo se guarda en el `localStorage` del navegador.

## Cómo jugar

1. Abre `index.html` en el navegador (o sírvelo con cualquier servidor estático).
2. Crea tu nombre de jugador en la barra superior.
3. En la pestaña **Quiniela**, elige la ronda y apuesta el pódium de la **sprint**
   y de la **carrera** (1º, 2º y 3º).
4. Solo puedes crear o modificar tu apuesta de **lunes a miércoles**, la semana
   del Gran Premio. De jueves a domingo la quiniela de esa ronda queda bloqueada.
5. Tras el fin de semana, cualquiera puede introducir el resultado oficial en
   la pestaña **Resultados (Admin)**. En ese momento se calculan los puntos de
   todos los jugadores para esa ronda.
6. La pestaña **Clasificación** muestra el ranking acumulado.

## Puntuación

| | Posición exacta | Piloto en pódium, mal puesto | Pódium perfecto |
|---|---|---|---|
| Sprint | 2 pts/puesto | 1 pt | +3 pts bonus |
| Carrera | 3 pts/puesto | 1 pt | +5 pts bonus |

## Estructura

```
motogp-quiniela/
  index.html        # esqueleto de la SPA
  css/style.css      # tema visual (oscuro, acentos MotoGP)
  js/data.js         # equipos, pilotos y calendario (editable)
  js/storage.js      # localStorage + cálculo de la ventana de apuestas
  js/scoring.js       # reglas de puntuación y clasificación
  js/app.js           # renderizado de vistas y eventos
```

## Datos editables

Las alineaciones de equipos y el calendario 2026 en `js/data.js` son
**orientativos**. Si hay fichajes o cambios de fechas confirmados, edita
directamente los arrays `TEAMS` y `CIRCUITS` — no hace falta tocar el resto
del código.

## Notas

- No hay servidor ni cuentas: cada jugador se identifica solo por su nombre.
- Los datos viven en el navegador de cada persona; para jugar en grupo, cada
  jugador necesita abrir la app en su propio dispositivo (o compartir el mismo
  perfil de navegador).
