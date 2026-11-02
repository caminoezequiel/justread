# Offline English → Spanish dictionary

`en-es.tsv.gz` is embedded in the app (`src/dictionary.rs`) and is what the
translation popup shows for a clicked word or a selection.

## Source and license

Built from the English Wiktionary (https://en.wiktionary.org), as extracted by
[wiktextract](https://github.com/tatuylonen/wiktextract) and published at
[kaikki.org](https://kaikki.org/dictionary/English/). Wiktionary content is
available under the [Creative Commons Attribution-ShareAlike 4.0
license](https://creativecommons.org/licenses/by-sa/4.0/); so is this file,
which is a derived subset (senses with Spanish translations, plus inflected
forms). The app's code stays MIT; the popup credits Wiktionary.

Current file: kaikki.org extract of 2026-10-30, built 2026-11-01, about
57,000 headwords (including phrasal verbs and idioms), 2.5 MB.

## Format

UTF-8, tab-separated, one line per sense, sorted by headword (lowercase):

```text
run	verb	to move quickly on two feet	correr|apeonar
run out	verb	to use up	agotar|quedarse sin|acabarse
ran	=	run
```

`=` lines map an inflected form to its base forms (`better	=	good|well`).

## Rebuilding

```bash
python3 reader-app/dictionary/build.py                 # streams ~530 MB from kaikki.org
python3 reader-app/dictionary/build.py English.jsonl.gz # or from a downloaded copy
```

The script only needs the Python standard library. Commit the new
`en-es.tsv.gz` together with any change to `build.py`.
