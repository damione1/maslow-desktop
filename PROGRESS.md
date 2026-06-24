# Maslow Desktop Client — Progression

App desktop Mac/Windows (Rust + Tauri + SvelteKit) qui pilote la Maslow M4 via
l'API réseau du firmware (HTTP + WebSocket). Réimplémentation moderne de l'UI web
embarquée (`ESP3D-WEBUI`), pour s'affranchir des contraintes mémoire de l'ESP32.

**État courant** : Phase 2 terminée (code) | Prochaine étape : Phase 3 — Maslow state machine (MINFO) + courroies | À valider : `npm run tauri dev` connecté à 192.168.0.106 (jog, hold/resume, streamer un .nc + couper le wifi → reprise, browse/run/delete SD)

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
- [x] JogControls (X/Y/Z jog `$J=G91`, pas 0.1/1/10/50 + feed, home `$H`, unlock `$X`, zero `G10 L20 P0`, jog cancel 0x85) — `JogControls.svelte`, verrouillé pendant un job
- [x] Realtime hold `!` (0x21) / resume `~` (0x7e) / reset `0x18` — boutons UI + backend `send_realtime`
- [x] Streaming G-code (char-counting GRBL 127 o) **+ résumable** : job possédé par le superviseur (survit aux reconnexions auto), progression persistée sur disque (`current_job.json`) → reprise après crash/déco via `stream_saved` → `stream_start(start_index=acked)`. Console verrouillée pendant un job (sinon les `ok` fausseraient le compteur). Tests Rust : char-counting, ack/complete, resume index, parsing gcode.
- [x] Gestionnaire fichiers SD : `upload_file` (POST multipart ESP3D `path`/`<full>S`/`myfile[]`), `list_files`, `delete_file` + `FileBrowser.svelte` (navigation dossiers, Run via `$SD/Run=`, delete, upload). _Rename : reporté (pas prioritaire)._

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

## En suspens / à valider sur hardware
- [ ] **Cycle de reprise** : streamer un .nc, couper le WiFi → vérifier passage en `interrupted` + reprise correcte à `acked` (rejeu de 1-2 lignes acceptable, jamais de saut).
- [ ] **État modal après reconnexion** : après une vraie coupure, FluidNC peut perdre G54/unités/position. Si le test révèle un décalage à la reprise → injecter un préambule modal (`G21 G90 G54 ...`) avant de reprendre le flux. À décider selon comportement réel.
- [ ] **Pause = feed-hold `!`** : valider que le mouvement s'arrête vite et que `~` + pump reprend proprement (acks en attente après `!`).
- [ ] **Reset ⌃X pendant un job** : on invalide le suivi (`invalidate_inflight`) ; vérifier que l'état UI passe bien `interrupted` et que la reprise repart juste.
- [ ] Rename SD (reporté, non prioritaire).
- [ ] Single-active-client : afficher si un autre client (UI web embarquée) prend la main via `ACTIVE_ID` (event `ws-pageid` déjà émis, pas encore exploité dans l'UI).

## Phases ultérieures (hors périmètre "jusqu'à Levenberg")
- Phase 5 — Auth/login, OTA firmware (`/updatefw`), préférences, packaging signé Mac/Windows, auto-update.

## Journal
- 2026-06-24 — Phase 0 : scaffold Tauri+SvelteKit, deps Rust, écran connexion (`ping_machine`). Connexion validée sur 192.168.0.106.
- 2026-06-24 — Phase 1 : `grbl.rs` (parser status, 4 tests), `connection.rs` (WS manager port 81, reconnect/watchdog), stores + StatusPanel + Console. Protocole confirmé via websocat sur la vraie machine.
- 2026-06-24 — Phase 2 (streaming+upload) : `streaming.rs` (Job char-counting, parsing gcode, persistance disque, 4 tests), intégration dans `connection.rs` (job possédé par `connection_loop`, ack→pump, interruption sur déco, commandes `stream_start/pause/resume/stop/saved`), `http_api.rs` (`upload_file`/`list_files`/`delete_file`), plugin dialog, store `job.ts` + `JobPanel.svelte` (barre de progression, reprise, upload). 8 tests Rust verts, svelte-check 0 erreur.
- 2026-06-24 — Phase 2 (contrôle+SD) : `JogControls.svelte` (jog XYZ + home/unlock/zero/jog-cancel + realtime hold/resume/reset), `FileBrowser.svelte` (navigation SD, run/delete), layout 2 colonnes. Phase 2 complète (rename SD reporté). svelte-check 0 erreur.
