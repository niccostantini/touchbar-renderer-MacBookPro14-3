# touchbar-renderer

Renderer per la Touch Bar dei MacBook Pro con chip T1 (2016–2017) su Linux,
sopra [t1bridge](https://github.com/standardagents/t1bridge), con slider in
stile macOS e controlli contestuali. Provato su MacBookPro14,3 con
Ubuntu 26.04 e GNOME 50.

Il procedimento completo (firmware T1, t1bridge su Ubuntu, problemi e
soluzioni) è in **[docs/PROCESSO.md](docs/PROCESSO.md)**.

## La barra

```
[esc] [📷][🔒][🌙][🎙️]   Titolo · Artista ▁▁▁▁▁   [☀][⌨]  [⏮][⏯][⏭]  [🔇][🔊]
```

- **Esc** sempre a sinistra; con **Fn** premuto: Esc, F1–F12.
- **📷** apre lo strumento screenshot di GNOME (tasto Stamp).
- **🔒** blocca lo schermo (`loginctl lock-session`).
- **🌙** Non disturbare di GNOME; illuminato quando attivo, anche se
  attivato dal pannello di GNOME.
- **🎙️** compare solo mentre un'app registra; rosso e barrato se muto.
- **Brano in riproduzione** (MPRIS): tocca per pausa, trascina per spostarti.
- **☀ ⌨ 🔊** aprono uno slider a tutta barra: tocca, oppure tocca e
  trascina subito per regolare in un gesto. Chiusura con ✕, Fn o dopo 4 s.
- A schermo bloccato o spento la barra è nera e non risponde.

## Installazione

Serve t1bridge installato (vedi `docs/PROCESSO.md`). Il renderer dipende dai
crate di t1bridge tramite percorso, quindi `t1bridge` va clonato **accanto**
a questo repository (`setup/01-build-t1bridge.sh` lo fa da solo):

```
~/t1bridge/            # t1bridge con setup/t1bridge-local.patch applicata
~/touchbar-renderer/   # questo repository
```

```sh
sh setup/03-install-renderer.sh
```

Per tornare al renderer integrato di t1bridge:

```sh
rm ~/.config/t1bridge/renderer && systemctl --user restart t1-touchbar.service
```

## Struttura

| File | Ruolo |
|---|---|
| `src/session.rs` | Connessione al servizio hardware, ciclo principale, invio dei fotogrammi |
| `src/connection.rs` | Protocollo e negoziazione con `t1-touchbar-hw` |
| `src/bar.rs` | Layout, disegno e riconoscimento dei tocchi |
| `src/input.rs` | Tocchi → tap, slider, trascinamento sul brano, Touch ID |
| `src/slider.rs` | Stato degli slider, chiusura automatica, passi di luminosità |
| `src/context.rs` | Thread di contesto: player, volume, microfono, blocco schermo |
| `src/mpris.rs`, `src/audio.rs`, `src/screen.rs`, `src/system.rs` | Integrazione con GNOME/PipeWire via `busctl`, `wpctl`, `gsettings`, `loginctl` |
| `src/levels.rs` | Livelli di luminosità letti da sysfs |
| `src/text.rs` | Testo con i font di sistema (`fontdue`) |
| `src/art/` | Icone e forme; `stock_*` copiati da t1bridge (MIT) |
| `src/bin/preview.rs` | Disegna tutte le schermate in PNG: `cargo run --release --bin preview -- /tmp/anteprime` |
| `setup/` | Script di compilazione/installazione di t1bridge, patch locale, provider |

## Sviluppo

```sh
cargo test --release
cargo run --release --bin preview -- /tmp/anteprime   # controllo grafico senza hardware
sh setup/03-install-renderer.sh                       # prova sulla barra
journalctl --user -u t1-touchbar -f                   # log
```

Note di progetto:
- La luminosità dello schermo si regola solo a passi (tasti luminosità),
  perché GNOME 50 annulla le scritture dirette: lo slider impara la
  dimensione del passo e si ferma entro mezzo passo dal valore scelto.
- Tutti i comandi esterni girano su thread separati: il ciclo di disegno
  non aspetta mai un sottoprocesso.
- `panic = "abort"`: se il renderer si blocca, t1bridge passa al renderer
  integrato invece di lasciare la barra in uno stato a metà.

## Licenza

MIT. Parti derivate da t1bridge (MIT): vedi `LICENSE-t1bridge`.

## Tema desktop "Leo Camp"

Tema GNOME ispirato allo stile di brave.com/leo: crema e nero, serif da
rivista (Fraunces), card con bordo spesso e ombra netta, gradiente
magenta → arancio, accenti viola; variante scura automatica; dock in basso;
icone Papirus con cartelle magenta; sfondi generati.

```sh
sudo apt install gnome-shell-extensions papirus-icon-theme   # una volta
sh theme/install.sh                                          # poi logout/login
sh ~/.config/leocamp/restore.sh                              # per tornare com'era
```
