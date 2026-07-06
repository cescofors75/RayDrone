/*
 * Datos de la temporada MotoGP 2026.
 * Alineaciones y fechas orientativas en base a la información disponible al cierre
 * de esta build — revisa y ajusta este fichero si hay fichajes o cambios de
 * calendario confirmados por Dorna antes de cada Gran Premio.
 */

const TEAMS = [
  { id: "ducati", name: "Ducati Lenovo Team", bike: "Ducati Desmosedici GP26", color: "#cc0000",
    riders: [ { id: "r1", name: "Marc Márquez", number: 93 }, { id: "r2", name: "Francesco Bagnaia", number: 63 } ] },
  { id: "pramac", name: "Prima Pramac Yamaha", bike: "Yamaha YZR-M1", color: "#4b0082",
    riders: [ { id: "r3", name: "Jack Miller", number: 43 }, { id: "r4", name: "Miguel Oliveira", number: 88 } ] },
  { id: "aprilia", name: "Aprilia Racing", bike: "Aprilia RS-GP26", color: "#1c2f6b",
    riders: [ { id: "r5", name: "Jorge Martín", number: 1 }, { id: "r6", name: "Marco Bezzecchi", number: 72 } ] },
  { id: "ktm-factory", name: "Red Bull KTM Factory Racing", bike: "KTM RC16", color: "#ff6600",
    riders: [ { id: "r7", name: "Pedro Acosta", number: 37 }, { id: "r8", name: "Brad Binder", number: 33 } ] },
  { id: "yamaha", name: "Monster Energy Yamaha MotoGP", bike: "Yamaha YZR-M1", color: "#2b2b8f",
    riders: [ { id: "r9", name: "Fabio Quartararo", number: 20 }, { id: "r10", name: "Álex Rins", number: 42 } ] },
  { id: "honda-factory", name: "Honda HRC Castrol", bike: "Honda RC213V", color: "#e2001a",
    riders: [ { id: "r11", name: "Joan Mir", number: 36 }, { id: "r12", name: "Luca Marini", number: 10 } ] },
  { id: "lcr", name: "LCR Honda", bike: "Honda RC213V", color: "#8b8b8b",
    riders: [ { id: "r13", name: "Johann Zarco", number: 5 }, { id: "r14", name: "Somkiat Chantra", number: 35 } ] },
  { id: "gresini", name: "Gresini Racing MotoGP", bike: "Ducati Desmosedici GP26", color: "#00a3e0",
    riders: [ { id: "r15", name: "Álex Márquez", number: 73 }, { id: "r16", name: "Fermín Aldeguer", number: 54 } ] },
  { id: "vr46", name: "VR46 Racing Team", bike: "Ducati Desmosedici GP26", color: "#ffcc00",
    riders: [ { id: "r17", name: "Franco Morbidelli", number: 21 }, { id: "r18", name: "Fabio Di Giannantonio", number: 49 } ] },
  { id: "trackhouse", name: "Trackhouse Racing", bike: "Aprilia RS-GP26", color: "#d21f3c",
    riders: [ { id: "r19", name: "Raúl Fernández", number: 25 }, { id: "r20", name: "Ai Ogura", number: 79 } ] },
  { id: "tech3", name: "Red Bull Tech3 KTM", bike: "KTM RC16", color: "#003da5",
    riders: [ { id: "r21", name: "Enea Bastianini", number: 23 }, { id: "r22", name: "Maverick Viñales", number: 12 } ] },
];

// Lista plana de pilotos para los desplegables de la quiniela.
const RIDERS = TEAMS.flatMap(team =>
  team.riders.map(rider => ({ ...rider, team: team.name, teamId: team.id, color: team.color }))
);

function getRiderById(id) {
  return RIDERS.find(r => r.id === id);
}

/*
 * Calendario 2026 (orientativo). Cada ronda define la fecha del GP (domingo).
 * La ventana de apuestas se calcula automáticamente como el lunes-miércoles
 * de esa misma semana (ver js/storage.js -> getBettingWindow).
 */
const CIRCUITS = [
  { round: 1,  id: "qatar",      name: "Losail International Circuit", country: "Catar", flag: "🇶🇦", city: "Lusail", date: "2026-03-08", length: "5.380 km", corners: 16 },
  { round: 2,  id: "argentina",  name: "Termas de Río Hondo", country: "Argentina", flag: "🇦🇷", city: "Termas de Río Hondo", date: "2026-03-22", length: "4.806 km", corners: 14 },
  { round: 3,  id: "americas",   name: "Circuit of The Americas", country: "EE. UU.", flag: "🇺🇸", city: "Austin", date: "2026-04-05", length: "5.513 km", corners: 20 },
  { round: 4,  id: "portugal",   name: "Autódromo Internacional do Algarve", country: "Portugal", flag: "🇵🇹", city: "Portimão", date: "2026-04-19", length: "4.592 km", corners: 15 },
  { round: 5,  id: "jerez",      name: "Circuito de Jerez - Ángel Nieto", country: "España", flag: "🇪🇸", city: "Jerez de la Frontera", date: "2026-05-03", length: "4.423 km", corners: 13 },
  { round: 6,  id: "lemans",     name: "Circuit Bugatti", country: "Francia", flag: "🇫🇷", city: "Le Mans", date: "2026-05-17", length: "4.185 km", corners: 14 },
  { round: 7,  id: "silverstone",name: "Silverstone Circuit", country: "Reino Unido", flag: "🇬🇧", city: "Silverstone", date: "2026-05-31", length: "5.900 km", corners: 18 },
  { round: 8,  id: "mugello",    name: "Autodromo del Mugello", country: "Italia", flag: "🇮🇹", city: "Mugello", date: "2026-06-14", length: "5.245 km", corners: 15 },
  { round: 9,  id: "sachsenring",name: "Sachsenring", country: "Alemania", flag: "🇩🇪", city: "Hohenstein-Ernstthal", date: "2026-06-28", length: "3.671 km", corners: 13 },
  { round: 10, id: "assen",      name: "TT Circuit Assen", country: "Países Bajos", flag: "🇳🇱", city: "Assen", date: "2026-07-12", length: "4.542 km", corners: 18 },
  { round: 11, id: "austria",    name: "Red Bull Ring", country: "Austria", flag: "🇦🇹", city: "Spielberg", date: "2026-07-26", length: "4.318 km", corners: 10 },
  { round: 12, id: "hungria",    name: "Balaton Park Circuit", country: "Hungría", flag: "🇭🇺", city: "Balatonfőkajár", date: "2026-08-16", length: "4.087 km", corners: 14 },
  { round: 13, id: "catalunya",  name: "Circuit de Barcelona-Catalunya", country: "España", flag: "🇪🇸", city: "Montmeló", date: "2026-08-30", length: "4.657 km", corners: 14 },
  { round: 14, id: "misano",     name: "Misano World Circuit", country: "San Marino", flag: "🇸🇲", city: "Misano Adriático", date: "2026-09-13", length: "4.226 km", corners: 16 },
  { round: 15, id: "motegi",     name: "Twin Ring Motegi", country: "Japón", flag: "🇯🇵", city: "Motegi", date: "2026-09-27", length: "4.801 km", corners: 14 },
  { round: 16, id: "mandalika",  name: "Pertamina Mandalika Circuit", country: "Indonesia", flag: "🇮🇩", city: "Lombok", date: "2026-10-04", length: "4.310 km", corners: 17 },
  { round: 17, id: "phillip",    name: "Phillip Island Circuit", country: "Australia", flag: "🇦🇺", city: "Phillip Island", date: "2026-10-18", length: "4.448 km", corners: 12 },
  { round: 18, id: "buriram",    name: "Chang International Circuit", country: "Tailandia", flag: "🇹🇭", city: "Buriram", date: "2026-10-25", length: "4.554 km", corners: 12 },
  { round: 19, id: "sepang",     name: "Sepang International Circuit", country: "Malasia", flag: "🇲🇾", city: "Sepang", date: "2026-11-08", length: "5.543 km", corners: 15 },
  { round: 20, id: "valencia",   name: "Circuit Ricardo Tormo", country: "España", flag: "🇪🇸", city: "Cheste", date: "2026-11-22", length: "4.005 km", corners: 14 },
];
