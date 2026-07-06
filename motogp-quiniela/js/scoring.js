/* Reglas de puntuación de la quiniela. */

const POINTS = {
  race: { exact: 3, partial: 1, perfectBonus: 5 },
  sprint: { exact: 2, partial: 1, perfectBonus: 3 },
};

/**
 * Compara un pódium apostado (array de 3 IDs de piloto, en orden 1º-2º-3º)
 * contra el resultado oficial y devuelve el desglose de puntos.
 */
function scorePodium(predicted, result, rules) {
  if (!predicted || !result) return { points: 0, exactCount: 0, partialCount: 0, perfect: false };

  let exactCount = 0;
  let partialCount = 0;

  predicted.forEach((riderId, i) => {
    if (!riderId) return;
    if (riderId === result[i]) {
      exactCount += 1;
    } else if (result.includes(riderId)) {
      partialCount += 1;
    }
  });

  const perfect = exactCount === 3;
  let points = exactCount * rules.exact + partialCount * rules.partial;
  if (perfect) points += rules.perfectBonus;

  return { points, exactCount, partialCount, perfect };
}

/**
 * Calcula el total de puntos de una predicción de una ronda (sprint + carrera).
 */
function scoreRound(prediction, result) {
  const sprint = scorePodium(prediction?.sprint, result?.sprint, POINTS.sprint);
  const race = scorePodium(prediction?.race, result?.race, POINTS.race);
  return { sprint, race, total: sprint.points + race.points };
}

/**
 * Genera la clasificación general: suma de puntos por jugador a lo largo de
 * todas las rondas que ya tienen resultado oficial.
 */
function buildLeaderboard(circuits, players) {
  const results = getAllResults();
  const predictions = getAllPredictions();

  const table = players.map(player => {
    let total = 0;
    let roundsPlayed = 0;
    circuits.forEach(c => {
      const result = results[c.id];
      const prediction = predictions[c.id] && predictions[c.id][player];
      if (result && prediction) {
        total += scoreRound(prediction, result).total;
        roundsPlayed += 1;
      }
    });
    return { player, total, roundsPlayed };
  });

  table.sort((a, b) => b.total - a.total);
  return table;
}
