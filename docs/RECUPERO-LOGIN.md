# Se la schermata di accesso non compare

Dopo aver installato il tema Leo Camp per GDM, se all'avvio o al logout lo
schermo resta nero o vuoto:

1. Premi **Ctrl + Alt + F3**: compare un terminale di testo.
2. Scrivi il tuo **nome utente**, Invio, poi la **password** (non si vede
   mentre la scrivi), Invio.
3. Scrivi:

   ```
   sudo update-alternatives --auto gdm-theme.gresource && sudo systemctl restart gdm3
   ```

   e inserisci di nuovo la password.
4. Torna la schermata di accesso di Ubuntu. Se non torna da sola, premi
   **Ctrl + Alt + F1** (o F2).

Per togliere del tutto il tema, una volta entrata nel desktop:

```
sudo sh ~/touchbar-renderer/theme/gdm/install.sh --restore
```
