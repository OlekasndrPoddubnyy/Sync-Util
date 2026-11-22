# Dimensions Sync (Rust)

Versione Rust di Dimensions Sync: allinea una cartella di destinazione al contenuto di una cartella sorgente,
rispettando le regole di esclusione in stile `.gitignore`. Interfaccia grafica in [Slint](https://slint.dev).

## Compilazione

Servono la toolchain Rust (`rustup`) e i Visual Studio Build Tools con il carico di lavoro C++.

```sh
cargo build --release        # target/release/dimensions-sync.exe
cargo test                   # test del motore e della GUI
cargo test --release -- --ignored --nocapture large_tree   # misura su 20.000 file
```

## Struttura

```
src/
  lib.rs              libreria: solo il motore, senza dipendenze dalla GUI
  engine/
    ignore_rules.rs   regole di esclusione (.gitignore)
    sync.rs           allineamento delle cartelle (in parallelo)
    text_compare.rs   confronto dei file di testo ignorando fine riga e codifica
    log.rs            registro delle attività, scritto da qualsiasi thread
  main.rs             avvio dell'eseguibile
  gui/
    mod.rs            stato dell'applicazione e collegamento dei callback
    console.rs        console di log con ricerca
    files.rs          scheda "File" (un albero per esito)
    file_tree.rs      albero ordinato dei percorsi elaborati
    exclusions.rs     scheda "Esclusioni"
    drop.rs           drag & drop delle cartelle da Esplora risorse
    settings.rs       regole e tema salvati in ~/.dimensions-sync
    format.rs         numeri e tempi
ui/
  app.slint           finestra principale
  theme.slint         palette chiara/scura e misure comuni
  models.slint        strutture scambiate con Rust
  components/         pulsanti, card, banner, riquadri, schede, stato
  panels/             console, alberatura dei file, editor delle esclusioni
  icons/              icone SVG
```

## Differenze rispetto alla versione Java

- **Prestazioni:** cartelle e file vengono elaborati in parallelo. Due file di dimensione diversa sono considerati
  diversi senza leggerli, e i file di testo vengono letti meno volte. Le cartelle rimaste vuote vengono eliminate
  durante la stessa visita, senza una seconda scansione.
- **Cartelle con solo `.dm`:** vengono eliminate insieme alla `.dm` (in Java l'eliminazione falliva e interrompeva la pulizia).
- **Errori durante la pulizia:** vengono segnalati nel log e la sincronizzazione prosegue.
- **Collegamenti simbolici:** eliminando una cartella non si seguono i collegamenti al suo interno.
- **Console:** la ricerca evidenzia l'intera riga e "Copia" copia tutto il log; non si seleziona una parte del testo.
- **Impostazioni:** le regole stanno nello stesso file della versione Java (`~/.dimensions-sync/syncignore`), quindi
  sono condivise; il tema è salvato in `~/.dimensions-sync/settings` invece che nel registro di Windows.
