/* Persistencia en localStorage y cálculo de la ventana de apuestas (lunes-miércoles). */

const STORAGE_KEYS = {
  players: "motogp_players",
  currentPlayer: "motogp_current_player",
  predictions: "motogp_predictions",
  results: "motogp_results",
};

function loadJSON(key, fallback) {
  try {
    const raw = localStorage.getItem(key);
    return raw ? JSON.parse(raw) : fallback;
  } catch (e) {
    return fallback;
  }
}

function saveJSON(key, value) {
  localStorage.setItem(key, JSON.stringify(value));
}

// --- Jugadores -------------------------------------------------------------

function getPlayers() {
  return loadJSON(STORAGE_KEYS.players, []);
}

function addPlayer(name) {
  const players = getPlayers();
  if (!players.includes(name)) {
    players.push(name);
    saveJSON(STORAGE_KEYS.players, players);
  }
  setCurrentPlayer(name);
}

function getCurrentPlayer() {
  return localStorage.getItem(STORAGE_KEYS.currentPlayer) || null;
}

function setCurrentPlayer(name) {
  localStorage.setItem(STORAGE_KEYS.currentPlayer, name);
}

// --- Predicciones ------------------------------------------------------

function getAllPredictions() {
  return loadJSON(STORAGE_KEYS.predictions, {});
}

function getPrediction(round, player) {
  const all = getAllPredictions();
  return (all[round] && all[round][player]) || null;
}

function savePrediction(round, player, sprint, race) {
  const all = getAllPredictions();
  if (!all[round]) all[round] = {};
  all[round][player] = { sprint, race, savedAt: new Date().toISOString() };
  saveJSON(STORAGE_KEYS.predictions, all);
}

// --- Resultados oficiales ------------------------------------------------

function getAllResults() {
  return loadJSON(STORAGE_KEYS.results, {});
}

function getResult(round) {
  return getAllResults()[round] || null;
}

function saveResult(round, sprint, race) {
  const all = getAllResults();
  all[round] = { sprint, race, savedAt: new Date().toISOString() };
  saveJSON(STORAGE_KEYS.results, all);
}

// --- Ventana de apuestas (lunes a miércoles de la semana del GP) -----------

/**
 * Dada la fecha del GP (domingo, "YYYY-MM-DD"), devuelve el rango
 * [lunes 00:00, miércoles 23:59:59] de esa misma semana.
 */
function getBettingWindow(raceDateStr) {
  const raceDate = new Date(raceDateStr + "T00:00:00");
  const monday = new Date(raceDate);
  monday.setDate(raceDate.getDate() - 6);
  monday.setHours(0, 0, 0, 0);
  const wednesday = new Date(raceDate);
  wednesday.setDate(raceDate.getDate() - 4);
  wednesday.setHours(23, 59, 59, 999);
  return { open: monday, close: wednesday };
}

/**
 * Estado de una ronda respecto a la ventana de apuestas:
 * "upcoming" (aún no abre), "open" (lunes-miércoles), "closed" (jueves en
 * adelante, en espera de resultados), "finished" (ya hay resultado oficial).
 */
function getRoundStatus(circuit, now = new Date()) {
  if (getResult(circuit.id)) return "finished";
  const { open, close } = getBettingWindow(circuit.date);
  if (now < open) return "upcoming";
  if (now >= open && now <= close) return "open";
  return "closed";
}

function formatDateRange(open, close) {
  const opts = { weekday: "long", day: "numeric", month: "long" };
  return `${open.toLocaleDateString("es-ES", opts)} - ${close.toLocaleDateString("es-ES", opts)}`;
}
