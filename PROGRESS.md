# Maslow Desktop Client — Progression

App desktop Mac/Windows (Rust + Tauri + SvelteKit) qui pilote la Maslow M4 via
l'API réseau du firmware (HTTP + WebSocket). Réimplémentation moderne de l'UI web
embarquée (`ESP3D-WEBUI`), pour s'affranchir des contraintes mémoire de l'ESP32.

**État courant** : Phase 0 — scaffold | Prochaine étape : écran connexion + test ping | Blocages : aucun

---

## Phase 0 — Scaffold
- [x] Nouveau dossier `/Users/damien/Projects/maslow-desktop` séparé du firmware
- [x] `create-tauri-app` template SvelteKit + TS
- [x] Cargo deps (tokio, tokio-tungstenite, reqwest, futures-util, uuid, serde)
- [x] `npm install`
- [x] `git init` + premier commit
- [ ] Écran connexion (host `maslow.local`/IP + test ping HTTP)

## Phase 1 — Connexion temps-réel + status
- [ ] WebSocket `/ws` (sous-protocole arduino) + reconnect + watchdog 20s
- [ ] Parser GRBL status report `<...>` (grbl.rs)
- [ ] Events Tauri → store Svelte → StatusPanel
- [ ] Console brute des lignes WS

## Phase 2 — Contrôle + fichiers
- [ ] JogControls (X/Y/Z, home `$H`, unlock `$X`)
- [ ] Realtime hold `!` / resume `~` / reset `0x18`
- [ ] Streaming G-code (buffer protocol, ~40 cmd en vol)
- [ ] Gestionnaire fichiers SD (list/upload/delete/rename)

## Phase 3 — Maslow state machine + courroies
- [ ] Polling MINFO (`$Maslow/getInfo`) + parsing
- [ ] Mapping 10 états + boutons contextuels
- [ ] BeltStatus (tl/tr/bl/br + erreurs etl/etr/ebl/ebr)
- [ ] Overrides moteur manuels (TLI/TRO…)

## Phase 4 — Calibration Levenberg + visualisation
- [ ] Workflow calibration piloté (retract → extend → calibrate)
- [ ] Suivi des waypoints `[MSG:INFO: Waypoint N…]`
- [ ] Visualisation toolpath/waypoints (canvas)
- [ ] Écran config Maslow (anchors `kinematics/MaslowKinematics/*`, work area, tension)
- [ ] (optionnel) Solver Levenberg-Marquardt client-side

---

## Phases ultérieures (hors périmètre "jusqu'à Levenberg")
- Phase 5 — Auth/login, OTA firmware (`/updatefw`), préférences, packaging signé Mac/Windows, auto-update.

## Journal
- 2026-06-24 — Phase 0 démarrée : scaffold Tauri+SvelteKit, deps Rust ajoutées, projet initialisé.
