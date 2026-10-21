# Passaggio alla GPU Intel (MacBookPro14,3)

Di serie il firmware avvia il Mac sulla Radeon, che pilota lo schermo interno e
resta sempre accesa. La variabile EFI `gpu-power-prefs` (`0` = Radeon,
`1` = Intel, la stessa di macOS) fa partire il Mac sulla Intel e lascia
dormire la Radeon. Prima del passaggio: 27,3 W a batteria (21/10/2026, con Brave + Claude).

Strumenti: `gpu-pref` (senza `--apply` non scrive mai niente) e
`gpu-pref-watchdog.service` (il paracadute).

## Livelli di recupero, dal più comodo

1. **Il desktop funziona ma qualcosa non va:** `sudo gpu-pref set dgpu --apply`, poi riavvia.
2. **Schermo nero ma Linux parte:** non fare niente. Se entro 5 minuti dall'avvio
   nessuno lancia `sudo gpu-pref confirm`, il watchdog rimette la Radeon e riavvia
   da solo, una volta sola. Puoi anche spegnere e riaccendere: il paracadute resta
   armato finché non scatta o non viene confermato.
3. **Il watchdog non basta** (Linux si blocca prima): reset della NVRAM. Spegni,
   accendi e tieni subito premuti ⌥ ⌘ P R per circa 20 secondi (fino al secondo
   suono di avvio o al secondo logo Apple). Il Mac torna sulla Radeon e parte da
   `EFI/BOOT/BOOTX64.EFI`. Effetti collaterali: volume, risoluzione e disco di
   avvio tornano ai valori di serie (shim ricrea la voce Ubuntu). La T1 non c'entra:
   il suo firmware è sulla partizione EFI.
4. **Ultima spiaggia:** chiavetta USB live di Ubuntu (tieni premuto ⌥ all'avvio), poi
   `sudo chattr -i /sys/firmware/efi/efivars/gpu-power-prefs-*` e
   `printf '\007\000\000\000\000\000\000\000' | sudo tee /sys/firmware/efi/efivars/gpu-power-prefs-fa4ce28d-b62f-4c99-9cc3-6815686e30f9`

## Fasi

- **A. Installazione** (la GPU non cambia): `sudo sh desktop/igpu/install.sh`
- **B. Prova di scrittura innocua:** `sudo gpu-pref set dgpu --apply`. Riscrive lo
  stesso valore di adesso (Radeon): verifica che il firmware accetti la scrittura
  (attributi `0x07` al posto di `0x80000007`) e crea il backup in
  `/var/lib/gpu-pref/original.bin`.
- **C. Prova del paracadute, ancora sulla Radeon:** `sudo gpu-pref arm --apply`,
  riavvia, **non** confermare. Dopo 5 minuti deve riavviarsi da solo, e
  `gpu-pref log` deve mostrare "nessuna conferma … riavvio".
- **D. Passaggio vero:** `sudo gpu-pref set igpu --apply`, poi `sudo gpu-pref arm --apply`,
  poi riavvia. Se va tutto bene, entro 5 minuti: `sudo gpu-pref confirm`.
- **E. Verifiche:** `gpu-pref status` (schermo su `i915`, Radeon `suspended`),
  luminosità dalla Touch Bar e dai tasti, webcam, assorbimento in W.

Indietro del tutto: `sudo gpu-pref set dgpu --apply`, poi `sudo sh desktop/igpu/install.sh --restore`.

## Da sapere

- Le porte Thunderbolt sono collegate alla Radeon: con un monitor esterno la
  Radeon si risveglia (GNOME la usa come seconda GPU). È previsto.
