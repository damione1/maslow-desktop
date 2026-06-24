# Maslow Desktop Client — Progression

App desktop Mac/Windows (Rust + Tauri + SvelteKit) qui pilote la Maslow M4 via
l'API réseau du firmware (HTTP + WebSocket). Réimplémentation moderne de l'UI web
embarquée (`ESP3D-WEBUI`), pour s'affranchir des contraintes mémoire de l'ESP32.

**État courant** : Phase 1 terminée (code) | Prochaine étape : Phase 2 — jog + realtime + fichiers SD | À valider : `npm run tauri dev` connecté à 192.168.0.106

> Découvertes machine réelle (FluidNC v1.21, Maslow M4 @ 192.168.0.106) :
> - WebSocket = `ws://<host>:81/` (PAS `/ws` → 404). Web port + 1.
> - Status reporte **5 axes** : `MPos:X,Y,Z,A,B`.
> - Ligne status contient `[GC:...]` après le `>` → parser extrait le `<...>`.

---

## Phase 0 — Scaffold
- [x] Nouveau dossier `/Users/damien/Projects/maslow-desktop` séparé du firmware
- [x] `create-tauri-app` template SvelteKit + TS
- [x] Cargo deps (tokio, tokio-tungstenite, reqwest, futures-util, uuid, serde)
- [x] `npm install`
- [x] `git init` + premier commit
- [x] Écran connexion (host `maslow.local`/IP + test ping HTTP via `ping_machine`)

## Phase 1 — Connexion temps-réel + status
- [x] WebSocket `ws://host:81/` (sous-protocole arduino) + reconnect 3s + watchdog 20s + polling `?` 250ms
- [x] Parser GRBL status report `<...>` 5 axes + `[GC:]` (grbl.rs, 4 tests OK)
- [x] Events Tauri (`machine-status`, `grbl-line`, `ws-state`) → stores Svelte → StatusPanel
- [x] Console des lignes WS (filtre les status reports, auto-scroll)

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
- 2026-06-24 — Phase 0 : scaffold Tauri+SvelteKit, deps Rust, écran connexion (`ping_machine`). Connexion validée sur 192.168.0.106.
- 2026-06-24 — Phase 1 : `grbl.rs` (parser status, 4 tests), `connection.rs` (WS manager port 81, reconnect/watchdog), stores + StatusPanel + Console. Protocole confirmé via websocat sur la vraie machine.
