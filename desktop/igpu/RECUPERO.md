# Recupero: prova amdgpu.runpm=1 (21/10/2026)

## Situazione al momento del riavvio di prova

- Variabile EFI `gpu-power-prefs` = Intel (`07 00 00 80 01 00 00 00`). Confermata, funziona.
  Il paracadute `gpu-pref` è disarmato.
- `/boot/grub/custom.cfg` contiene la voce di prova `gpu-prova` (Ubuntu + `amdgpu.runpm=1`).
- `/boot/grub/grubenv` ha `next_entry=gpu-prova`: **solo** il prossimo avvio usa la voce di
  prova, poi GRUB cancella la scelta e torna la voce normale.
- `/etc/default/grub` e `grub.cfg` NON sono stati modificati.

## Sintomi e rimedi

**A. Si blocca (all'avvio o mentre lo usi)**
Tieni premuto il tasto di accensione per 5 secondi e riaccendi. Riparte la voce normale. Se compare
il menu di GRUB, aspetta circa 30 secondi senza toccare niente: la tastiera interna lì non va.

**B. La voce di prova riparte a ogni avvio** (GRUB non è riuscito a cancellare la scelta)
- Se il desktop si apre: `sudo ~/touchbar-renderer/desktop/igpu/grub-prova rimuovi`
- Se si blocca ogni volta: chiavetta live (tieni premuto ⌥ all'accensione, "EFI Boot", "Prova Ubuntu"), poi:
  ```
  sudo mount /dev/nvme0n1p2 /mnt
  sudo rm /mnt/boot/grub/custom.cfg
  sudo grub-editenv /mnt/boot/grub/grubenv unset next_entry
  sudo umount /mnt
  ```
  Togli la chiavetta e riavvia. Senza `custom.cfg` la voce di prova non esiste più.

**C. Schermo nero anche con la voce normale** (non c'entra runpm: problema della GPU Intel)
1. Aspetta 10 minuti (il paracadute è disarmato, quindi non scatta da solo).
2. Reset della NVRAM: spegni, riaccendi tenendo premuti ⌥ ⌘ P R per circa 20 secondi. Il Mac torna sulla Radeon.
3. Altrimenti, dalla chiavetta live:
   ```
   sudo chattr -i /sys/firmware/efi/efivars/gpu-power-prefs-*
   printf '\007\000\000\000\000\000\000\000' | sudo tee /sys/firmware/efi/efivars/gpu-power-prefs-fa4ce28d-b62f-4c99-9cc3-6815686e30f9 >/dev/null
   ```

**Tornare del tutto come prima (tutto riprende a funzionare come prima)**
```
sudo ~/touchbar-renderer/desktop/igpu/grub-prova rimuovi
sudo gpu-pref set dgpu --apply      # torna alla Radeon (effetto al riavvio)
sudo sh ~/touchbar-renderer/desktop/igpu/install.sh --restore
```

## Da NON fare
- Non toccare `applespi` (driver di tastiera e trackpad) né i moduli della Touch Bar.
- Niente `update-grub` o modifiche a `/etc/default/grub` per "sistemare": non servono.
- Niente `rm` nella cartella `/sys/firmware/efi/efivars` (si rischia di danneggiare il firmware).

## Prompt per una nuova conversazione con Claude

> Sto lavorando sul risparmio energetico del mio MacBookPro14,3 (Ubuntu 26.04, kernel 7.0).
> Ho spostato l'avvio sulla GPU Intel (variabile EFI gpu-power-prefs) e poi ho fatto un avvio di
> prova con `amdgpu.runpm=1` tramite una voce GRUB in /boot/grub/custom.cfg scelta con next_entry.
> Leggi prima ~/touchbar-renderer/desktop/igpu/RECUPERO.md e PROTOCOLLO.md, e la memoria
> igpu-switch. Sintomo attuale: [DESCRIVI COSA SUCCEDE]. Verifica lo stato con comandi di sola
> lettura (gpu-pref status, cat /proc/cmdline, journalctl -b -1 -k | grep -i amdgpu,
> grub-editenv /boot/grub/grubenv list) prima di proporre qualsiasi modifica, e spiegami come
> tornare indietro prima di ogni comando con sudo. Non toccare applespi.
