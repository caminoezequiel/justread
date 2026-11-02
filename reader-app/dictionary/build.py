#!/usr/bin/env python3
"""Build en-es.tsv.gz, the offline English -> Spanish dictionary.

Source: English Wiktionary as extracted by wiktextract (https://kaikki.org),
CC BY-SA 4.0. Usage (from anywhere):

    python3 build.py [kaikki.org-dictionary-English.jsonl.gz]

Without an argument the file is streamed from kaikki.org (about 530 MB).
Output lines (UTF-8, tab-separated, sorted by headword):

    word  pos  sense   es1|es2|...   one sense with Spanish translations
    form  =    lemma1|lemma2         an inflected form (ran -> run)

Headwords are lowercase; multi-word entries (phrasal verbs, idioms) are kept.
"""

import gzip
import json
import sys
import urllib.request
from pathlib import Path

URL = "https://kaikki.org/dictionary/English/kaikki.org-dictionary-English.jsonl.gz"
OUT = Path(__file__).resolve().parent / "en-es.tsv.gz"
MAX_SENSE = 80  # characters of the English sense description
MAX_WORDS = 6  # Spanish translations per sense
MAX_SENSES = 12  # senses per word and part of speech
# Parts of speech worth looking up while reading.
SKIP_POS = {"name", "character", "symbol", "suffix", "prefix", "infix", "affix"}
# Forms not worth redirecting a modern reader to ("book" as an old past of "bake").
SKIP_FORM_TAGS = {"obsolete", "archaic", "dialectal", "misspelling", "nonstandard", "rare", "pronunciation-spelling"}


def clean(text):
    return " ".join(str(text).replace("|", "/").split())


def short(text):
    text = clean(text)
    return text if len(text) <= MAX_SENSE else text[: MAX_SENSE - 1].rstrip() + "…"


def add_form(forms, form, lemma):
    lemmas = forms.setdefault(form, [])
    if lemma not in lemmas:
        lemmas.append(lemma)


def open_source():
    if len(sys.argv) > 1:
        return gzip.open(sys.argv[1], "rt", encoding="utf-8")
    print(f"downloading {URL}", file=sys.stderr)
    return gzip.open(urllib.request.urlopen(URL), "rt", encoding="utf-8")


def main():
    senses = {}  # (word, pos) -> {sense: [es words]}, in first-seen order
    forms = {}  # form -> [lemmas]
    with open_source() as source:
        for line in source:
            entry = json.loads(line)
            word = clean(entry.get("word", "")).lower()
            pos = entry.get("pos", "")
            if not word or pos in SKIP_POS or "\t" in word:
                continue
            groups = senses.setdefault((word, pos), {})
            # Translation tables: on the entry (labelled with a short sense
            # description) or on a sense (then its gloss describes them).
            tables = [(t.get("sense"), t) for t in entry.get("translations") or []]
            for sense in entry.get("senses") or []:
                gloss = (sense.get("glosses") or [None])[0]
                for t in sense.get("translations") or []:
                    tables.append((t.get("sense") or gloss, t))
                if SKIP_FORM_TAGS.intersection(sense.get("tags") or []):
                    continue
                for target in sense.get("form_of") or sense.get("alt_of") or []:
                    lemma = clean(target.get("word", "")).lower()
                    if lemma and lemma != word:
                        add_form(forms, word, lemma)
            for label, t in tables:
                if t.get("lang_code", t.get("code")) != "es" or not t.get("word"):
                    continue
                es = clean(t["word"])
                words = groups.setdefault(short(label or "—"), [])
                if es not in words and len(words) < MAX_WORDS:
                    words.append(es)
            # Inflections listed on the lemma itself.
            for form in entry.get("forms") or []:
                text = clean(form.get("form", "")).lower()
                tags = form.get("tags") or []
                if (
                    text
                    and text != word
                    and " " not in text
                    and "table-tags" not in tags
                    and not SKIP_FORM_TAGS.intersection(tags)
                ):
                    add_form(forms, text, word)

    known = {word for (word, _), groups in senses.items() if groups}
    lines = []
    for (word, pos), groups in senses.items():
        for sense, words in list(groups.items())[:MAX_SENSES]:
            lines.append(f"{word}\t{pos}\t{sense}\t{'|'.join(words)}")
    for form, lemmas in forms.items():
        # Only lemmas that have translations.
        lemmas = [lemma for lemma in lemmas if lemma in known]
        if lemmas:
            lines.append(f"{form}\t=\t{'|'.join(lemmas[:3])}")
    # Stable order: headword, then the original order of its lines.
    lines = sorted(enumerate(lines), key=lambda p: (p[1].split("\t", 1)[0], p[0]))
    with gzip.open(OUT, "wt", encoding="utf-8", compresslevel=9) as out:
        out.write("# English Wiktionary via kaikki.org, CC BY-SA 4.0. Built by build.py.\n")
        for _, line in lines:
            out.write(line + "\n")
    print(f"{len(known)} words, {len(lines)} lines -> {OUT} ({OUT.stat().st_size} bytes)", file=sys.stderr)


if __name__ == "__main__":
    main()
