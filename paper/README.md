# paper/ — hacia un artículo (DAFx)

Esta carpeta es para convertir RayDrone en un **artículo científico**. La idea,
en una frase: *la síntesis granular asíncrona es un estimador Monte Carlo de una
integral de transporte sobre el eje temporal, igual que un píxel en la ecuación
de rendering — y las técnicas de reducción de varianza de los gráficos transfieren
y mejoran la convergencia de forma medible.*

- **`raydrone-dafx.md`** — borrador/esqueleto del paper (en inglés, con notas 💡
  en español que borraremos antes de enviar).

## Tus dudas, respondidas en corto

**¿Esto es publicable de verdad o me estoy flipando?**
Es publicable. No porque "suene bonito", sino porque la analogía es *medible*: el
Convergence Lab ya muestra el 1/√N y que las estrategias mejoran la pendiente. Eso
es una contribución concreta y reproducible. Lo que NO hay que afirmar es que
simulas física acústica (no lo haces) — y como lo decimos nosotros en la sección
de límites, jugamos limpio.

**¿No es "solo" síntesis granular renombrada?**
No. La síntesis granular existe; lo nuevo es el **marco** (verla como estimación
MC) y el **resultado** (que el toolkit de varianza de gráficos transfiere y se
mide). Ese cambio de lente es exactamente el tipo de aportación que valora DAFx.

**¿Y si un revisor me dice que la analogía es superficial?**
Por eso la Sección 3 pone las DOS fórmulas (`g[n]` y `ĝ_N[n]`): no es una metáfora,
es literalmente el mismo estimador. Y la Sección 5.3 (trazado inverso sesgado)
demuestra que entendemos la diferencia entre converger mejor y sonar distinto.

## Dónde enviarlo
- **DAFx** — encaje natural (efectos/síntesis digital). *(primera opción)*
- **SMC** (Sound and Music Computing), **ICMC** (computer music).
- *Computer Music Journal* (MIT Press) para versión extendida.

## Qué falta (checklist)
- [ ] **Export CSV del Convergence Lab** (curvas RMS vs N por estrategia) para
      generar las figuras del paper de forma reproducible.
- [ ] Correr el Lab sobre 2–3 samples distintos (bajo 303, pad, percusión) y
      tabular las pendientes ajustadas.
- [ ] Fig. 1: diagrama píxel/rayo ↔ foco/grano (ya tenemos el texto "genesis").
- [ ] Fig. 2: distribuciones de puntos de las 4 estrategias sobre la apertura.
- [ ] Fig. 3: gráfica log-log de convergencia con la línea ideal 1/√N.
- [ ] Fig. 4: Reverse (se aplana) vs Importance (converge) → sesgo.
- [ ] Redactar Sec. 3 (núcleo matemático) en limpio.
- [ ] Pasar a plantilla LaTeX de DAFx cuando el contenido esté cerrado.

> Sin prisa y con rigor. El código y el Lab ya son nuestra "sección de
> reproducibilidad"; el resto es contar bien lo que ya hicimos.
