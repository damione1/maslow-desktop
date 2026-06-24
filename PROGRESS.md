# Maslow Desktop Client — Progression

App desktop Mac/Windows (Rust + Tauri + SvelteKit) qui pilote la Maslow M4 via
l'API réseau du firmware (HTTP + WebSocket). Réimplémentation moderne de l'UI web
embarquée (`ESP3D-WEBUI`), pour s'affranchir des contraintes mémoire de l'ESP32.

**État courant** : Phase 4 en cours — state machine firmware + waypoints + viz canvas (code) | Prochaine étape : workflow calibration guidé (wizard) + écran config Maslow (anchors/work area) | À valider : `npm run tauri dev` (lancer une calibration, voir les waypoints se tracer)

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
- [x] Polling MINFO (`$Maslow/getInfo`) + `$Maslow/gstate` toutes les 1.5 s **uniquement hors job** (sinon le `ok` fausserait le char-counting) ; `maslow.rs` parse MINFO (homed/calibrationInProgress/tl/tr/br/bl/etl/etr/ebr/ebl/extended) + `[MSG:INFO: Current state: N]`. 4 tests.
- [x] Mapping 10 états (0-9 MaslowEnums.h) + boutons contextuels (`MaslowPanel.svelte`) répliquant `updateDynamicButtons()` : retract `[0,2,4,7]`, extend/comply `[0,2,4]`, takeSlack/calibrate `[4]`. Stop/E-Stop. Badges homed/extended/calibrating, animation "busy" états 1/3/5/6/9.
- [x] BeltStatus tl/tr/bl/br (longueurs mm) + erreurs etl/etr/ebl/ebr (surlignées si |err|>1 mm), grille 4 coins. Filtrage des `ok` de polling dans la console.
- [ ] Overrides moteur manuels (TLI/TRO…) — reporté (debug avancé, non prioritaire).

> Note : commandes Maslow (`$Maslow/...`) désactivées pendant un job en cours ; l'arrêt d'urgence en cours de coupe reste le Reset ⌃X realtime (toujours dispo).

## Phase 4 — Calibration Levenberg + visualisation
- [x] **State machine propre (source de vérité firmware)** : `maslow.rs::policy_for()` encode la matrice état→actions dérivée de `Calibration::requestStateChange()`. Émet `maslow-state` = `{ code, label, busy, allowed[] }`. Le front (`MaslowPanel`) ne fait que lire `allowed`/`busy` (plus aucune règle dupliquée). 4 tests policy. **Voir section dédiée ci-dessous.**
- [x] Suivi des waypoints `[MSG:INFO: Waypoint N coordinates: X=.. Y=..]` → `maslow.rs::parse_waypoint` → event `maslow-waypoint` → store `waypoints` (reset auto en entrant en état 6).
- [x] Visualisation waypoints (canvas auto-scalé, dernier point surligné, indicateur live) — `CalibrationView.svelte`. _Toolpath G-code (rendu du fichier .nc) : à faire._
- [ ] Workflow calibration **guidé** (wizard retract → extend → takeSlack/calibrate avec étapes/prérequis) — boutons contextuels OK, wizard pas-à-pas à faire.
- [ ] Écran config Maslow (anchors `kinematics/MaslowKinematics/*`, work area, tension) via `$/<key>`.
- [ ] (optionnel) Solver Levenberg-Marquardt client-side.

### State machine Maslow — matrice firmware (source : `Calibration::requestStateChange`)
Tous les handlers de commande utilisent `anyState` ; le **vrai** gating est dans `requestStateChange(newState)`. Actions utilisateur → état cible → états source autorisés :
- **Retract** → RETRACTING → **tout état**
- **Extend** → EXTENDING → RETRACTED(2), EXTENDEDOUT(4)
- **Take Slack** → TAKING_SLACK → EXTENDEDOUT(4), READY_TO_CUT(7)
- **Calibrate** → CALIBRATION_IN_PROGRESS → EXTENDEDOUT(4), READY_TO_CUT(7), COMPUTING(9)
- **Comply** → RELEASE_TENSION → UNKNOWN(0), EXTENDEDOUT(4), READY_TO_CUT(7), COMPUTING(9)
- **Stop / E-Stop** → tout état
- Transitions internes (non déclenchables par l'utilisateur) : RETRACTED←RETRACTING, EXTENDEDOUT←(EXTENDING/TAKING_SLACK/RELEASE_TENSION/COMPUTING/IN_PROGRESS), COMPUTING←IN_PROGRESS, READY_TO_CUT←(IN_PROGRESS/COMPUTING/TAKING_SLACK).

> **À construire (demandé) — state machine robuste** : les changements d'état remontés par le firmware sont capricieux (rapports bruités/sauts). Notre `policy_for` filtre déjà les actions aux **états stables** (busy = 1/3/5/6/8/9 → Stop/E-Stop seulement) pour la prévisibilité. Reste à durcir : (1) valider les transitions entrantes contre le graphe firmware pour ignorer/lisser les rapports impossibles, (2) éventuel debounce des états transitoires. NB : ce n'est PAS une FSM qu'on pilote (le firmware possède l'état) — c'est une **fonction état→policy** + un validateur de transitions ; une lib FSM classique n'apporte rien ici (voir note lib dans le journal).

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
- 2026-06-24 — Phase 3 (Maslow state machine + courroies) : `maslow.rs` (parse MINFO + état, 4 tests), polling 1.5 s hors job dans `connection.rs`, events `maslow-info`/`maslow-state`, store `maslow.ts`, `MaslowPanel.svelte` (état + boutons contextuels + courroies). 12 tests Rust verts, svelte-check 0 erreur.
- 2026-06-24 — Fix commandes Maslow : formes courtes `$MINFO`/`$GSTATE`/`$ALL`/`$EXT`/`$CMP`/`$TKSLK`/`$CAL`/`$STOP`/`$ESTOP` (les formes longues `$Maslow/...` renvoient error:3). Télémétrie validée sur la vraie machine.
- 2026-06-24 — Phase 4 (state machine firmware + waypoints) : `policy_for()` (matrice dérivée de `requestStateChange`, 4 tests), `parse_waypoint` (1 test), events `maslow-state` enrichi + `maslow-waypoint`/`maslow-cal-complete`, `CalibrationView.svelte` (canvas waypoints). 15 tests Rust verts, svelte-check 0 erreur.
  - **Note lib state machine** (demandée) : pas de lib FSM (Rust `statig`/`rust-fsm`, JS XState) car le firmware **possède** l'état — on ne pilote pas de transitions, on mappe `état→actions`. Une FSM classique suppose qu'on dirige les transitions ; ici c'est une table de policy + (à venir) un validateur de graphe. La capriciosité se règle par validation/lissage des rapports, pas par une lib.
