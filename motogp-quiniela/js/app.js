/* Controlador principal de la SPA: navegación, render de vistas y eventos. */

const state = {
  view: "quiniela",
  selectedRound: null,
};

const $app = document.getElementById("app");
const $playerBar = document.getElementById("player-bar");

function init() {
  ensureDefaultPlayer();
  state.selectedRound = pickDefaultRound();
  renderPlayerBar();
  renderNav();
  render();
  setInterval(() => render(), 60 * 1000); // refresca estados de ventana cada minuto
}

function ensureDefaultPlayer() {
  if (!getCurrentPlayer() && getPlayers().length === 0) {
    // sin jugadores todavía: se pedirá alta en la barra superior
  }
}

function pickDefaultRound() {
  const now = new Date();
  const open = CIRCUITS.find(c => getRoundStatus(c, now) === "open");
  if (open) return open.id;
  const upcoming = CIRCUITS.find(c => getRoundStatus(c, now) === "upcoming");
  if (upcoming) return upcoming.id;
  return CIRCUITS[CIRCUITS.length - 1].id;
}

// --- Barra de jugador --------------------------------------------------

function renderPlayerBar() {
  const players = getPlayers();
  const current = getCurrentPlayer();
  $playerBar.innerHTML = `
    <div class="player-bar-inner">
      <span class="brand">🏁 Quiniela MotoGP 2026</span>
      <div class="player-controls">
        ${players.length ? `
          <select id="player-select">
            ${players.map(p => `<option value="${escapeHtml(p)}" ${p === current ? "selected" : ""}>${escapeHtml(p)}</option>`).join("")}
          </select>
        ` : `<span class="muted">Sin jugadores todavía</span>`}
        <input id="new-player-input" type="text" placeholder="Nuevo jugador..." maxlength="20" />
        <button id="add-player-btn">Añadir</button>
      </div>
    </div>
  `;

  document.getElementById("add-player-btn").addEventListener("click", () => {
    const input = document.getElementById("new-player-input");
    const name = input.value.trim();
    if (name) {
      addPlayer(name);
      renderPlayerBar();
      render();
    }
  });

  const select = document.getElementById("player-select");
  if (select) {
    select.addEventListener("change", e => {
      setCurrentPlayer(e.target.value);
      render();
    });
  }
}

// --- Navegación ----------------------------------------------------------

const NAV_ITEMS = [
  { id: "quiniela", label: "🎯 Quiniela" },
  { id: "calendario", label: "📅 Calendario" },
  { id: "clasificacion", label: "🏆 Clasificación" },
  { id: "equipos", label: "🏍️ Equipos" },
  { id: "circuitos", label: "📍 Circuitos" },
  { id: "resultados", label: "✅ Resultados (Admin)" },
  { id: "reglas", label: "📖 Reglas" },
];

function renderNav() {
  const nav = document.getElementById("main-nav");
  nav.innerHTML = NAV_ITEMS.map(
    item => `<button class="nav-btn" data-view="${item.id}">${item.label}</button>`
  ).join("");

  nav.querySelectorAll(".nav-btn").forEach(btn => {
    btn.addEventListener("click", () => {
      state.view = btn.dataset.view;
      render();
    });
  });
}

function updateNavActive() {
  document.querySelectorAll(".nav-btn").forEach(btn => {
    btn.classList.toggle("active", btn.dataset.view === state.view);
  });
}

// --- Render principal ------------------------------------------------------

function render() {
  updateNavActive();
  switch (state.view) {
    case "quiniela": return renderQuiniela();
    case "calendario": return renderCalendario();
    case "clasificacion": return renderClasificacion();
    case "equipos": return renderEquipos();
    case "circuitos": return renderCircuitos();
    case "resultados": return renderResultadosAdmin();
    case "reglas": return renderReglas();
    default: return renderQuiniela();
  }
}

// --- Vista: Quiniela -------------------------------------------------------

function riderOptions(selectedId) {
  return `<option value="">-- Elegir piloto --</option>` + RIDERS.map(
    r => `<option value="${r.id}" ${r.id === selectedId ? "selected" : ""}>#${r.number} ${escapeHtml(r.name)} (${escapeHtml(r.team)})</option>`
  ).join("");
}

function renderQuiniela() {
  const currentPlayer = getCurrentPlayer();

  if (!currentPlayer) {
    $app.innerHTML = `<div class="card"><p>👆 Crea o selecciona un jugador arriba para poder apostar.</p></div>`;
    return;
  }

  const roundSelectHtml = `
    <select id="round-select">
      ${CIRCUITS.map(c => `<option value="${c.id}" ${c.id === state.selectedRound ? "selected" : ""}>Ronda ${c.round} - ${c.flag} ${escapeHtml(c.name)}</option>`).join("")}
    </select>
  `;

  const circuit = CIRCUITS.find(c => c.id === state.selectedRound);
  const status = getRoundStatus(circuit);
  const { open, close } = getBettingWindow(circuit.date);
  const existing = getPrediction(circuit.id, currentPlayer);
  const result = getResult(circuit.id);

  const statusBadge = {
    upcoming: `<span class="badge badge-upcoming">Aún no abierta</span>`,
    open: `<span class="badge badge-open">🟢 ABIERTA - puedes apostar</span>`,
    closed: `<span class="badge badge-closed">🔒 Cerrada (fuera de lunes-miércoles)</span>`,
    finished: `<span class="badge badge-finished">🏁 Finalizada</span>`,
  }[status];

  $app.innerHTML = `
    <div class="card">
      <div class="round-picker">
        <label>Elige ronda:</label>
        ${roundSelectHtml}
      </div>
      <h2>${circuit.flag} ${escapeHtml(circuit.name)} — ${escapeHtml(circuit.city)}, ${escapeHtml(circuit.country)}</h2>
      <p class="muted">GP: ${new Date(circuit.date + "T00:00:00").toLocaleDateString("es-ES", { weekday: "long", day: "numeric", month: "long", year: "numeric" })}</p>
      <p>Ventana de apuestas: <strong>${formatDateRange(open, close)}</strong></p>
      <p>${statusBadge}</p>

      <form id="quiniela-form" class="quiniela-form">
        <fieldset ${status !== "open" ? "disabled" : ""}>
          <legend>🏃 Sprint - Pódium</legend>
          <div class="podium-row">
            <div class="podium-slot"><label>🥇 1º</label><select name="sprint-1">${riderOptions(existing?.sprint?.[0])}</select></div>
            <div class="podium-slot"><label>🥈 2º</label><select name="sprint-2">${riderOptions(existing?.sprint?.[1])}</select></div>
            <div class="podium-slot"><label>🥉 3º</label><select name="sprint-3">${riderOptions(existing?.sprint?.[2])}</select></div>
          </div>

          <legend>🏆 Carrera - Pódium</legend>
          <div class="podium-row">
            <div class="podium-slot"><label>🥇 1º</label><select name="race-1">${riderOptions(existing?.race?.[0])}</select></div>
            <div class="podium-slot"><label>🥈 2º</label><select name="race-2">${riderOptions(existing?.race?.[1])}</select></div>
            <div class="podium-slot"><label>🥉 3º</label><select name="race-3">${riderOptions(existing?.race?.[2])}</select></div>
          </div>

          <button type="submit">${existing ? "Actualizar apuesta" : "Guardar apuesta"}</button>
        </fieldset>
      </form>
      <div id="form-msg"></div>

      ${result ? renderRoundResultSummary(circuit, existing, result) : ""}
    </div>
  `;

  document.getElementById("round-select").addEventListener("change", e => {
    state.selectedRound = e.target.value;
    render();
  });

  const form = document.getElementById("quiniela-form");
  if (form && status === "open") {
    form.addEventListener("submit", e => {
      e.preventDefault();
      const fd = new FormData(form);
      const sprint = [fd.get("sprint-1"), fd.get("sprint-2"), fd.get("sprint-3")];
      const race = [fd.get("race-1"), fd.get("race-2"), fd.get("race-3")];

      if (hasDuplicates(sprint) || hasDuplicates(race)) {
        document.getElementById("form-msg").innerHTML = `<p class="error">⚠️ No puedes repetir el mismo piloto en un pódium.</p>`;
        return;
      }
      if (sprint.some(v => !v) || race.some(v => !v)) {
        document.getElementById("form-msg").innerHTML = `<p class="error">⚠️ Completa los 3 puestos de sprint y carrera.</p>`;
        return;
      }

      savePrediction(circuit.id, currentPlayer, sprint, race);
      document.getElementById("form-msg").innerHTML = `<p class="success">✅ Apuesta guardada para ${escapeHtml(currentPlayer)}.</p>`;
    });
  }
}

function hasDuplicates(arr) {
  const filled = arr.filter(Boolean);
  return new Set(filled).size !== filled.length;
}

function renderRoundResultSummary(circuit, prediction, result) {
  if (!prediction) {
    return `<div class="result-summary"><p class="muted">No hiciste una apuesta para esta ronda.</p></div>`;
  }
  const score = scoreRound(prediction, result);
  return `
    <div class="result-summary">
      <h3>Resultado oficial</h3>
      <p><strong>Sprint:</strong> ${result.sprint.map(id => getRiderById(id)?.name).join(" → ")}</p>
      <p><strong>Carrera:</strong> ${result.race.map(id => getRiderById(id)?.name).join(" → ")}</p>
      <p class="points-total">Puntos obtenidos: <strong>${score.total}</strong> (Sprint: ${score.sprint.points} · Carrera: ${score.race.points})</p>
    </div>
  `;
}

// --- Vista: Calendario -------------------------------------------------

function renderCalendario() {
  const now = new Date();
  $app.innerHTML = `
    <div class="card">
      <h2>📅 Calendario 2026</h2>
      <p class="muted">Fechas orientativas: ajusta <code>js/data.js</code> si Dorna confirma cambios.</p>
      <div class="calendar-grid">
        ${CIRCUITS.map(c => {
          const status = getRoundStatus(c, now);
          const { open, close } = getBettingWindow(c.date);
          const statusLabel = {
            upcoming: "Próximamente",
            open: "🟢 Abierta",
            closed: "🔒 Cerrada",
            finished: "🏁 Finalizada",
          }[status];
          return `
            <div class="calendar-card status-${status}">
              <span class="round-number">Ronda ${c.round}</span>
              <h3>${c.flag} ${escapeHtml(c.name)}</h3>
              <p class="muted">${escapeHtml(c.city)}, ${escapeHtml(c.country)}</p>
              <p>${new Date(c.date + "T00:00:00").toLocaleDateString("es-ES", { day: "numeric", month: "short", year: "numeric" })}</p>
              <p class="muted small">Apuestas: ${formatDateRange(open, close)}</p>
              <span class="badge">${statusLabel}</span>
            </div>
          `;
        }).join("")}
      </div>
    </div>
  `;
}

// --- Vista: Clasificación ---------------------------------------------

function renderClasificacion() {
  const players = getPlayers();
  if (players.length === 0) {
    $app.innerHTML = `<div class="card"><p>Todavía no hay jugadores registrados.</p></div>`;
    return;
  }
  const table = buildLeaderboard(CIRCUITS, players);

  $app.innerHTML = `
    <div class="card">
      <h2>🏆 Clasificación general</h2>
      <table class="leaderboard">
        <thead><tr><th>#</th><th>Jugador</th><th>Rondas puntuadas</th><th>Puntos</th></tr></thead>
        <tbody>
          ${table.map((row, i) => `
            <tr class="${row.player === getCurrentPlayer() ? "current-player" : ""}">
              <td>${i + 1}</td>
              <td>${escapeHtml(row.player)}</td>
              <td>${row.roundsPlayed}</td>
              <td><strong>${row.total}</strong></td>
            </tr>
          `).join("")}
        </tbody>
      </table>
    </div>
  `;
}

// --- Vista: Equipos ------------------------------------------------------

function renderEquipos() {
  $app.innerHTML = `
    <div class="card">
      <h2>🏍️ Equipos y pilotos 2026</h2>
      <p class="muted">Alineaciones orientativas: revisa <code>js/data.js</code> ante fichajes confirmados.</p>
      <div class="teams-grid">
        ${TEAMS.map(team => `
          <div class="team-card" style="border-top-color:${team.color}">
            <h3>${escapeHtml(team.name)}</h3>
            <p class="muted">${escapeHtml(team.bike)}</p>
            <ul>
              ${team.riders.map(r => `<li>#${r.number} ${escapeHtml(r.name)}</li>`).join("")}
            </ul>
          </div>
        `).join("")}
      </div>
    </div>
  `;
}

// --- Vista: Circuitos ----------------------------------------------------

function renderCircuitos() {
  $app.innerHTML = `
    <div class="card">
      <h2>📍 Circuitos 2026</h2>
      <div class="circuits-grid">
        ${CIRCUITS.map(c => `
          <div class="circuit-card">
            <span class="round-number">Ronda ${c.round}</span>
            <h3>${c.flag} ${escapeHtml(c.name)}</h3>
            <p class="muted">${escapeHtml(c.city)}, ${escapeHtml(c.country)}</p>
            <p>Longitud: ${escapeHtml(c.length)} · Curvas: ${c.corners}</p>
          </div>
        `).join("")}
      </div>
    </div>
  `;
}

// --- Vista: Resultados (Admin) -------------------------------------------

function renderResultadosAdmin() {
  const roundOptions = CIRCUITS.map(c => `<option value="${c.id}" ${c.id === state.selectedRound ? "selected" : ""}>Ronda ${c.round} - ${c.flag} ${escapeHtml(c.name)}</option>`).join("");
  const circuit = CIRCUITS.find(c => c.id === state.selectedRound);
  const existingResult = getResult(circuit.id);
  const status = getRoundStatus(circuit);

  $app.innerHTML = `
    <div class="card">
      <h2>✅ Introducir resultados oficiales</h2>
      <p class="muted">Aquí se registra el pódium real de sprint y carrera para calcular puntos automáticamente. Solo disponible una vez cerrada la ventana de apuestas.</p>
      <div class="round-picker">
        <label>Ronda:</label>
        <select id="admin-round-select">${roundOptions}</select>
      </div>

      ${status === "open" || status === "upcoming" ? `
        <p class="warning">⚠️ La ventana de apuestas de esta ronda todavía no ha cerrado. Espera al jueves para introducir resultados.</p>
      ` : `
        <form id="result-form">
          <fieldset>
            <legend>🏃 Sprint - Pódium real</legend>
            <div class="podium-row">
              <div class="podium-slot"><label>🥇 1º</label><select name="sprint-1">${riderOptions(existingResult?.sprint?.[0])}</select></div>
              <div class="podium-slot"><label>🥈 2º</label><select name="sprint-2">${riderOptions(existingResult?.sprint?.[1])}</select></div>
              <div class="podium-slot"><label>🥉 3º</label><select name="sprint-3">${riderOptions(existingResult?.sprint?.[2])}</select></div>
            </div>
            <legend>🏆 Carrera - Pódium real</legend>
            <div class="podium-row">
              <div class="podium-slot"><label>🥇 1º</label><select name="race-1">${riderOptions(existingResult?.race?.[0])}</select></div>
              <div class="podium-slot"><label>🥈 2º</label><select name="race-2">${riderOptions(existingResult?.race?.[1])}</select></div>
              <div class="podium-slot"><label>🥉 3º</label><select name="race-3">${riderOptions(existingResult?.race?.[2])}</select></div>
            </div>
            <button type="submit">${existingResult ? "Actualizar resultado" : "Guardar resultado"}</button>
          </fieldset>
        </form>
        <div id="admin-msg"></div>
      `}
    </div>
  `;

  document.getElementById("admin-round-select").addEventListener("change", e => {
    state.selectedRound = e.target.value;
    render();
  });

  const form = document.getElementById("result-form");
  if (form) {
    form.addEventListener("submit", e => {
      e.preventDefault();
      const fd = new FormData(form);
      const sprint = [fd.get("sprint-1"), fd.get("sprint-2"), fd.get("sprint-3")];
      const race = [fd.get("race-1"), fd.get("race-2"), fd.get("race-3")];

      if (hasDuplicates(sprint) || hasDuplicates(race)) {
        document.getElementById("admin-msg").innerHTML = `<p class="error">⚠️ No puedes repetir el mismo piloto en un pódium.</p>`;
        return;
      }
      if (sprint.some(v => !v) || race.some(v => !v)) {
        document.getElementById("admin-msg").innerHTML = `<p class="error">⚠️ Completa los 3 puestos de sprint y carrera.</p>`;
        return;
      }

      saveResult(circuit.id, sprint, race);
      document.getElementById("admin-msg").innerHTML = `<p class="success">✅ Resultado guardado. Se han recalculado los puntos.</p>`;
    });
  }
}

// --- Vista: Reglas ---------------------------------------------------------

function renderReglas() {
  $app.innerHTML = `
    <div class="card">
      <h2>📖 Reglas de la quiniela</h2>
      <h3>Ventana de apuestas</h3>
      <p>Cada Gran Premio se disputa de viernes a domingo. Las apuestas para esa ronda solo pueden
      hacerse (o modificarse) el <strong>lunes, martes y miércoles</strong> anteriores. A partir del
      jueves la quiniela de esa ronda queda bloqueada.</p>

      <h3>Puntuación</h3>
      <table class="rules-table">
        <thead><tr><th></th><th>Acertar posición exacta</th><th>Piloto en el pódium pero posición equivocada</th><th>Pódium perfecto (bonus)</th></tr></thead>
        <tbody>
          <tr><td>Sprint</td><td>${POINTS.sprint.exact} pts / puesto</td><td>${POINTS.sprint.partial} pt</td><td>+${POINTS.sprint.perfectBonus} pts</td></tr>
          <tr><td>Carrera</td><td>${POINTS.race.exact} pts / puesto</td><td>${POINTS.race.partial} pt</td><td>+${POINTS.race.perfectBonus} pts</td></tr>
        </tbody>
      </table>
      <p class="muted">Máximo posible por ronda: ${POINTS.sprint.exact * 3 + POINTS.sprint.perfectBonus} pts en sprint + ${POINTS.race.exact * 3 + POINTS.race.perfectBonus} pts en carrera.</p>

      <h3>Jugadores</h3>
      <p>No hay contraseñas: cada jugador se identifica por su nombre. Los datos se guardan en el
      navegador (localStorage), así que para compartir la quiniela con amigos cada uno debe abrir
      la app en su propio dispositivo/navegador.</p>
    </div>
  `;
}

// --- Utilidades ------------------------------------------------------------

function escapeHtml(str) {
  if (str === null || str === undefined) return "";
  return String(str)
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

document.addEventListener("DOMContentLoaded", init);
