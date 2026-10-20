# Touch Bar su MacBook Pro 14,3 con Ubuntu 26.04: il procedimento completo

Cronaca ragionata di come la Touch Bar è passata da spenta (T1 in Recovery
Mode) a funzionante con un renderer personalizzato, con gli errori fatti
lungo la strada perché non vengano ripetuti.

Sistema di riferimento: MacBookPro14,3 (15", 2017, chip Apple T1),
Ubuntu 26.04.1, kernel `7.0.0-38-generic`, GNOME 50 su Wayland,
systemd 259, Rust 1.93 dai pacchetti Ubuntu.

## 0. Le due cose da sapere prima di tutto

1. **`applespi` è il driver di tastiera e trackpad integrati** (sono su bus
   SPI). Non ha nulla a che fare con la Touch Bar. Metterlo in blacklist o
   rimuoverlo lascia il Mac senza tastiera e trackpad (è successo: si
   recupera togliendo la riga dalla blacklist con una tastiera USB).
   I driver della vecchia Touch Bar sono invece `apple_ibridge`,
   `apple_ib_tb`, `apple_ib_als` (repository `macbook12-spi-driver`).
2. **La Touch Bar moderna su Linux è t1bridge**, non `macbook12-spi-driver`.
   Il vecchio stack va disattivato: blocca il T1 nella configurazione USB
   sbagliata e impedisce sia la rigenerazione del firmware sia t1bridge.

## 1. Rigenerare il firmware del T1 (t1-revive)

Sintomo: `lsusb` mostra `05ac:1281 Apple Mobile Device [Recovery Mode]`
invece di `05ac:8600 iBridge`. Succede quando la partizione EFI di macOS
(con `EFI/APPLE/EMBEDDEDOS`) è stata cancellata installando Linux.

Strumento: [t1-revive](https://github.com/niconistal/t1-revive) (provato
sul 14,3). Passi seguiti:

```sh
git clone https://github.com/niconistal/t1-revive ~/t1-revive
cd ~/t1-revive && bash build.sh
sudo apt install acpi-call-dkms && sudo modprobe acpi_call
sudo bin/t1-revive preflight     # deve restare solo il NO sui pacchetti "Arch-only"
sudo bin/t1-revive regenerate    # ~5 minuti, alimentatore collegato
```

Il preflight segnala il vecchio stack Touch Bar: vanno tolti il DKMS
`apple-ib-drv`, la regola udev `99-ibridge.rules` (anche la copia `.bak`)
e qualunque servizio che carichi `apple_ibridge`.

Dopo `regenerate` i file sono in `/boot/efi/EFI/APPLE/EMBEDDEDOS/`
(`combined.memboot`, `FDRData`, `version.plist`): il T1 li carica da solo a
ogni accensione. **Fare subito un backup fuori dal disco, cifrato**
(`FDRData` identifica il T1 ed è indispensabile per Touch ID):

```sh
mkdir -p ~/t1-backup && sudo ~/t1-revive/bin/t1-revive backup --to ~/t1-backup
sudo tar -C ~/t1-backup -czf - . | gpg -c -o ~/t1-backup-EFI_APPLE.tar.gz.gpg
# poi copiare il .gpg su cloud/chiavetta e conservare la password
```

A questo punto la barra si accende solo finché t1-revive ha avviato il T1:
dopo un riavvio resta spenta finché non c'è t1bridge.

## 2. Compilare e installare t1bridge su Ubuntu

[t1bridge](https://github.com/standardagents/t1bridge) ha pacchetti solo
per Arch, quindi si compila dai sorgenti e si installa ricalcando i suoi
`PKGBUILD` (`packaging/arch/`). Dipendenze Ubuntu:

```sh
sudo apt install -y rustc cargo pkg-config libudev-dev libsystemd-dev libarchive-dev dkms
```

Poi, da questo repository:

```sh
sh setup/01-build-t1bridge.sh          # clona t1bridge accanto, applica la patch, compila
sudo sh setup/02-install-t1bridge.sh   # programmi, servizi, DKMS, initramfs
# spegnere del tutto, 20 s, riaccendere
```

Lo script d'installazione:
- installa programmi e servizi systemd/udev come farebbe il pacchetto Arch;
- installa via DKMS i moduli `t1_cfgsel`, `appletbdrm`, `apple_t1_ncm`,
  `uvcvideo` (firmati automaticamente con la chiave MOK, quindi funzionano
  con Secure Boot);
- aggiunge `t1_cfgsel` a `/etc/initramfs-tools/modules` (l'equivalente
  Ubuntu di `mkinitcpio`);
- impedisce il caricamento dei driver Touch Bar legacy (in conflitto);
- crea il gruppo `t1bridge` e attiva `t1-touchbar-hw` e il servizio utente
  `t1-touchbar`.

Disinstallazione completa: `sudo sh setup/uninstall-t1bridge.sh`.

### La patch locale (`setup/t1bridge-local.patch`)

Due modifiche al servizio hardware `t1-touchbar-hw`:

1. **Luminosità dello schermo.** GNOME 50 gestisce direttamente la
   retroilluminazione e annulla subito le scritture fatte via logind (scritto
   900, mezzo secondo dopo era di nuovo 1023). Il passo di luminosità
   schermo viene quindi inviato come tasto standard `KEY_BRIGHTNESSUP/DOWN`
   dalla tastiera virtuale di t1bridge: GNOME lo gestisce e mostra il suo
   indicatore.
2. **Tasto Stamp.** GNOME 50 rifiuta le richieste D-Bus di screenshot da
   programmi esterni ("Access denied"), quindi la tastiera virtuale può
   inviare anche `KEY_SYSRQ` (Stamp), che apre lo strumento di cattura.

Dopo una modifica: `sh setup/01-build-t1bridge.sh` e
`sudo sh setup/update-hw-service.sh` (conserva l'originale come `.orig`).

## 3. Problemi incontrati dopo l'installazione

| Sintomo | Causa | Soluzione |
|---|---|---|
| Tastiera e trackpad morti | `blacklist applespi` aggiunto per errore | togliere la riga da `/etc/modprobe.d/`, `modprobe applespi`, `update-initramfs -u` |
| Luminosità schermo dalla barra non funziona | GNOME 50 annulla le scritture logind | patch: tasti luminosità (sopra) |
| Fn non mostra F1–F12 | **keyd** catturava in esclusiva la tastiera SPI, nascondendo `KEY_FN` a t1bridge | `sudo systemctl disable --now keyd` |
| Il "tic" del volume non si sente | l'uscita audio predefinita era una dock USB (S/PDIF) | scegliere "Audio interno" in Impostazioni → Audio |
| Volume e tasti media non funzionano col renderer integrato | t1bridge richiede un "desktop provider" che non fornisce | `setup/t1bridge-desktop-provider` (vedi sotto) |

### Il desktop provider (solo per il renderer integrato)

`setup/t1bridge-desktop-provider` collega il renderer integrato di t1bridge
a GNOME: volume via `wpctl`, tasti media via MPRIS. Si installa in
`~/.local/bin/` e si attiva con un drop-in del servizio utente:

```ini
# ~/.config/systemd/user/t1-touchbar.service.d/provider.conf
[Service]
Environment=T1BRIDGE_DESKTOP_PROVIDER=%h/.local/bin/t1bridge-desktop-provider
```

Il renderer di questo repository **non lo usa**: t1bridge lo interroga ogni
secondo lanciando diversi processi (circa il 5% di un core). Resta utile se
si torna al renderer integrato.

## 4. Il renderer personalizzato (questo repository)

t1bridge permette di sostituire il renderer collegando un eseguibile in
`~/.config/t1bridge/renderer`. Gira come utente normale, senza sudo; se
termina, t1bridge torna da solo al renderer integrato.

```sh
sh setup/03-install-renderer.sh
```

Cosa fa in più rispetto al renderer integrato:
- **slider in stile macOS** per volume, luminosità schermo e luminosità
  tastiera (tocca per aprire, oppure tocca e trascina subito; chiusura con ✕,
  con Fn o dopo 4 s);
- tasti **Screenshot**, **Blocca schermo**, **Non disturbare**;
- **brano in riproduzione** con avanzamento (tocca = pausa, trascina = salto);
- tasto **microfono** che compare solo mentre un'app registra;
- barra **nera e inerte** a schermo bloccato o spento;
- consumo intorno allo 0,8% di un core (player e blocco a eventi D-Bus,
  microfono via `/proc/asound`, ciclo principale più lento a riposo).

Dettagli tecnici nel [README](../README.md).

## 5. Dove sono le cose sul sistema

| Cosa | Percorso |
|---|---|
| Firmware T1 | `/boot/efi/EFI/APPLE/EMBEDDEDOS/` |
| Programmi t1bridge | `/usr/lib/t1bridge/`, `/usr/bin/t1bridge` |
| Moduli kernel | DKMS `t1bridge-dkms` → `/lib/modules/$(uname -r)/updates/dkms/` |
| Driver legacy bloccati | `/etc/modprobe.d/blacklist-apple-ib.conf`, `/usr/lib/modprobe.d/t1bridge.conf` |
| Renderer scelto | `~/.config/t1bridge/renderer` (collegamento) |
| Log | `journalctl -u t1-touchbar-hw`, `journalctl --user -u t1-touchbar` |

## 6. Ancora da fare

- **Touch ID**: supportato da t1bridge, ma sostituisce `libfprint`/`fprintd`
  di sistema e tocca PAM. Da fare con una shell di root aperta e il login
  con password verificato.
- Tasti diversi per ogni applicazione: GNOME non espone l'app in primo piano
  ai programmi esterni; servirebbe una piccola estensione di GNOME Shell.
- Con Touch ID attivo, la barra nera a schermo bloccato nasconderebbe
  l'indicazione dell'impronta: andrà rivista.
