# Maslow Desktop Client — Progression

App desktop Mac/Windows (Rust + Tauri + SvelteKit) qui pilote la Maslow M4 via
l'API réseau du firmware (HTTP + WebSocket). Réimplémentation moderne de l'UI web
embarquée (`ESP3D-WEBUI`), pour s'affranchir des contraintes mémoire de l'ESP32.

**État courant** : Phase 4 en cours — state machine durcie + wizard guidé + **mode reprise quotidien** (Apply Tension → Ready to cut depuis EXTENDEDOUT/RETRACTED) + **badge « Calibré »** (lecture des ancrages `kinematics/*` via HTTP) + couverture 100 % des actions opérationnelles Maslow | Prochaine étape : écran config Maslow complet (édition anchors/work area/tension via `$/<key>`) | À valider : `npm run tauri dev` (au boot belts attachés → vérifier badge « Calibré ✓ » + carte « Reprise » qui lance `$TKSLK` → READY_TO_CUT sans recalibrer ; dérouler aussi le wizard complet première-fois)

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
- [x] **Policy unifiée + validation de transitions** : `action_policy(fluidnc_state, maslow_state, job_active)` réconcilie l'état machine FluidNC (gating motion) + l'état calibration Maslow + le job en cours en un seul jeu d'actions autorisées (event `action-policy`). `valid_transition()` (graphe firmware) + `StateTracker`/`Observation` lissent les rapports capricieux : straggler (transition impossible dans la fenêtre debounce 400 ms → ignoré) vs discord (hors fenêtre → la machine l'emporte, loggé via event `maslow-discord` dans la console). Front (`JogControls`/`MaslowPanel`) ne lit plus que `actionPolicy` (zéro règle dupliquée).
- [x] Workflow calibration **guidé** (wizard retract → extend → takeSlack/calibrate avec étapes/prérequis) — `CalibrationWizard.svelte` : stepper qui suit l'état firmware (avance tout seul), chaque action activée uniquement si la policy l'autorise, compteur waypoints live pendant CALIBRATION_IN_PROGRESS, Stop/E-Stop intégrés. Additif aux boutons contextuels du `MaslowPanel` (pas un remplacement).
- [x] **Mode reprise quotidien** : carte « Reprise » en tête du wizard quand la machine boote calibrée en EXTENDEDOUT(4) ou RETRACTED(2). Bouton proéminent adaptatif piloté par la policy : `take_slack` → « Reprendre — appliquer la tension » (`$TKSLK`, EXTENDEDOUT→READY_TO_CUT direct, vérifié dans `takeSlackFunc()`), sinon `extend` → « Reprendre — étendre les courroies » (`$EXT`, depuis RETRACTED). La séquence complète première-fois/récupération reste accessible (repliée par défaut, toggle « Calibration complète »).
- [x] **Badge « Calibré ✓ (ancrages en mémoire) »** : commande backend `read_maslow_anchors` (HTTP `$/kinematics/MaslowKinematics/`) → `maslow::parse_anchors` + `anchors_valid` (sanity géométrique alignée sur `checkBoundaries` firmware : top>bottom, left<right, largeur>0, non nul). Rafraîchi à la connexion et après `maslow-cal-complete`. Affiché dans le wizard + flag `calibré` du `MaslowPanel`. _NB : le firmware ne distingue pas « fraîchement calibré » de « valeurs par défaut » via la config — on signale uniquement « ancrages valides chargés »._
- [x] **Couverture 100 % des actions opérationnelles Maslow** : audit croisé `ProcessSettings.cpp` (handlers `$...`) ↔ UI. Déjà couvertes : retract `$ALL`, extend `$EXT`, takeSlack `$TKSLK`, calibrate `$CAL`, comply/release `$CMP`, stop `$STOP`, e-stop `$ESTOP`. Hors périmètre opérationnel (volontaire) : overrides moteur manuels `$TLI/$TRI/$BLI/$BRI/$TLO/...` (debug, reporté), `$CALRESET` (récupération de niche), `$SETZSTOP`/`$SFON`/`$SFOFF`/`$TELEM` (réglages/diagnostic).
- [ ] Écran config Maslow (édition anchors `kinematics/MaslowKinematics/*`, work area, tension) via `$/<key>` — la *lecture* des anchors est faite (badge), reste l'écriture/édition.
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

> **State machine robuste (FAIT)** : les changements d'état remontés par le firmware sont capricieux (rapports bruités/sauts). `policy_for` filtre les actions opérationnelles aux **états stables** (busy = 1/3/5/6/8/9), MAIS **Retract/Stop/E-Stop restent toujours autorisés** (calque fidèle du firmware : `requestStateChange` accepte RETRACTING depuis n'importe quel état, et `$STOP`/`$ESTOP` sont inconditionnels). C'est la **récupération d'un état bloqué** : `$STOP` arrête les moteurs et remet FluidNC en Idle sans réinitialiser la FSM Maslow → un Stop en EXTENDING(3) gèle l'état ; Retract est la seule sortie. Durcissement livré : (1) `valid_transition(from,to)` valide chaque rapport entrant contre le graphe firmware ; (2) `StateTracker` lisse via debounce 400 ms (straggler ignoré / discord accepté+loggé) ; (3) `action_policy` réconcilie FluidNC + Maslow + job en une policy unique, avec Retract découplé du gate `stable` ; (4) burst-poll `$GSTATE` post-action (~3 s @ 250 ms) pour que l'UI converge en < 1 s au lieu de 1,5 s. NB : ce n'est PAS une FSM qu'on pilote (le firmware possède l'état) — c'est une **fonction état→policy** + un validateur de transitions ; une lib FSM classique n'apporte rien ici (voir note lib dans le journal).

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
- 2026-06-24 — Phase 4 (reprise quotidienne + badge calibré + couverture actions) : `maslow.rs` enrichi de `Anchors`/`parse_anchors`/`anchors_valid` (lecture des ancrages depuis le dump config, validité géométrique alignée sur `checkBoundaries`), commande `read_maslow_anchors` (`http_api.rs`, HTTP `$/kinematics/MaslowKinematics/`) enregistrée dans `lib.rs`. Front : store `anchors` + `refreshAnchors()` (rafraîchi à la connexion via `ws-state` et après `maslow-cal-complete`), carte « Reprise » adaptative dans `CalibrationWizard.svelte` (bouton `$TKSLK`/`$EXT` selon la policy, séquence complète repliée par défaut + toggle), badge « Calibré ✓ (ancrages en mémoire) » dans le wizard + flag `calibré` du `MaslowPanel`. Vérifié dans le firmware : `$ALL`/`$EXT`/`$TKSLK`/`$CAL`/`$CMP`/`$STOP`/`$ESTOP` = couverture 100 % du jeu opérationnel ; `takeSlackFunc()` → `requestStateChange(READY_TO_CUT)` confirme le chemin court EXTENDEDOUT→READY_TO_CUT (pas de recalibration). MaslowEnums.h reconfirmé : 8=RELEASE_TENSION, 9=CALIBRATION_COMPUTING (les constantes Rust étaient déjà correctes). **+4 tests Rust (25 verts au total), svelte-check 0 erreur.** À valider sur hardware : au boot belts attachés (EXTENDEDOUT) → badge « Calibré ✓ » + carte « Reprise » qui mène en READY_TO_CUT via `$TKSLK` sans dérouler la grille.
- 2026-06-24 — Phase 4 (state machine durcie + wizard) : `maslow.rs` enrichi de `valid_transition` (graphe firmware), `StateTracker`/`Observation` (straggler vs discord, debounce 400 ms) et `action_policy` unifiée (FluidNC + Maslow + job → un seul jeu d'actions) ; `connection.rs` : `SocketCtx` (wco_cache + tracker + dernier état FluidNC + dernière policy), `route_line` remplace `dispatch_line`, events `action-policy` + `maslow-discord`, recompute de policy sur changement d'état / fin de job. Front : store `actionPolicy` + listeners, `JogControls`/`MaslowPanel` lisent la policy unifiée (plus de gating dupliqué), nouveau `CalibrationWizard.svelte` (stepper guidé piloté par l'état firmware). **6 tests Rust ajoutés (21 verts au total), svelte-check 0 erreur.** Fixes au passage : `JogControls` référençait encore `canMove` (supprimé) → `canUnlock`/`canZero` ; `unlock` (`$X`, commande ligne) sorti du groupe realtime always-on pour rester bloqué pendant un job (sinon corruption du char-counting). À valider sur hardware : dérouler le wizard de bout en bout.
- 2026-06-24 — **Fix bug « Stop bloque la machine » (vérifié firmware)** : un `$STOP` (`maslow_stop`) arrête les moteurs et remet FluidNC en Idle **mais ne réinitialise jamais la FSM de calibration** → un Stop pressé en EXTENDING(3) gèle `currentState` à EXTENDING indéfiniment. Or le firmware refuse Extend depuis EXTENDING (il faut RETRACTED/EXTENDEDOUT) **mais accepte Retract depuis n'importe quel état** : Retract est la seule sortie. Bug côté desktop : `policy_for()` plaçait `retract` derrière le gate `!busy`, donc on **désactivait la seule commande de récupération**, ne laissant que Stop/E-Stop ; et comme l'état firmware ne change plus après Stop, `StateTracker.observe` renvoie `Unchanged` et le pipeline event-on-change ne réactive jamais rien → « rien ne répond, ça traîne ». Correctifs :
  - **P0 (correctness)** `maslow.rs` : **Retract/Stop/E-Stop toujours autorisés** quel que soit `busy`, dans `policy_for()` (liste `allowed`) ET `action_policy()` (`retract` découplé du gate `stable`, tout en restant bloqué pendant un job via `!job_active`). extend/takeSlack/calibrate/comply restent gated (états source spécifiques). Tests : `policy_busy_allows_recovery_only` (réécrit), `policy_retract_allowed_while_extending`, `action_policy_stop_from_extending_recovers`, `action_policy_busy_allows_stop_and_retract` ; `action_policy_homing_locks_motion` ajusté (retract désormais vivant mid-op).
  - **P1 (latence)** `connection.rs` : après l'envoi d'une commande action Maslow (`$ALL/$EXT/$TKSLK/$CAL/$CMP/$STOP/$ESTOP`, via `is_maslow_action`), on tire immédiatement `$MINFO`+`$GSTATE` puis on **burst-poll `$GSTATE` (~12 × 250 ms ≈ 3 s)** au lieu d'attendre le tick 1,5 s → l'UI reflète le nouvel état firmware en < 1 s. Désactivé pendant un job (même règle que le poll 1,5 s, jamais de corruption du char-counting).
  - **P2 (UX)** affordance « stabilisation » : `MaslowPanel.svelte` met en évidence le bouton Retract (`.recover`) + ligne d'aide quand `busy` ; `CalibrationWizard.svelte` ajoute une ligne d'aide minimale (pas de conflit avec la carte Reprise) indiquant que Retract récupère un état transitoire.
  - **28 tests Rust verts (+3), `cargo build` OK, svelte-check 0 erreur/0 warning.** À valider sur hardware : Stop en plein EXTENDING → vérifier que Retract reste cliquable, récupère la machine, et que l'UI bascule en quelques centaines de ms (burst-poll).
