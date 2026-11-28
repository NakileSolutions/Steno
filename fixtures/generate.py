#!/usr/bin/env python3
"""Generates fixtures/daily-45min.jsonl: a made-up 45-minute daily, in French, in the
sidecar protocol format (docs/contracts.md §2). Deterministic (fixed seed).

Required content (PLAN §11): APP-… tickets, ms-booking service, one decision, two actions,
a voice command, a private sentence, trap words close to « Sténo » ("scénario", "stylo",
"sténographie"), an echo, and a command spoken by another participant (them channel).
"""
import json
import random
from datetime import datetime, timedelta, timezone
from pathlib import Path

random.seed(42)
TZ = timezone(timedelta(hours=1))
START = datetime(2026, 11, 26, 10, 30, 0, tzinfo=TZ)
DURATION = 45 * 60

events = []


def ts(t):
    return (START + timedelta(seconds=t)).isoformat(timespec="seconds")


def seg(t, channel, text, private=False, dur=None):
    d = dur if dur is not None else max(1.2, len(text.split()) * 0.38)
    events.append({"type": "segment", "ts": ts(t + d), "t0": round(t, 1), "t1": round(t + d, 1),
                   "channel": channel, "text": text, "private": private})
    return t + d


# Required scenes, at fixed times (seconds).
SCRIPTED = {
    30: [("them", "Bonjour à tous, on commence la daily ?"), ("me", "Bonjour, oui allons-y.")],
    120: [("them", "Hier j'ai avancé sur le APP-1234, le verrou de créneau côté ms-booking."),
          ("them", "Le problème c'est que le verrou n'est pas relâché quand la saga échoue.")],
    300: [("me", "Le scénario de migration de la table des créneaux est passé en recette hier soir."),
          ("them", "Sur le tableau, au stylo, on a listé les abonnements famille.")],
    480: [("them", "On regarde le ticket APP-1234 ensemble ?"),
          ("me@echo", "On regarde le ticket APP-1234 ensemble ?")],
    640: [("me", "Sténo, cherche les erreurs Datadog sur ms-booking depuis hier."),
          ("them", "Ah oui bonne idée, on a eu des 500 cette nuit.")],
    760: [("them", "Sténo, cherche le runbook de ms-pricing."),
          ("me", "Ah non, ça c'est moi qui déclenche, pas toi !")],
    900: [("them", "Pour l'export Brindille, le APP-1302, on attend toujours le format du fichier."),
          ("me", "Je m'occupe de mettre à jour la page Confluence sur l'export Brindille d'ici jeudi.")],
    1100: [("me@private", "Note pour moi : demander à Paul son avis pour l'entretien annuel de Julie."),
           ("me@private", "Et penser à réserver la salle pour vendredi.")],
    1300: [("them", "Donc on décide de garder le verrou de créneau côté ms-booking jusqu'à la v2, on ne le déplace pas dans le gateway."),
           ("me", "D'accord, décision actée : on garde le verrou dans ms-booking jusqu'à la v2.")],
    1500: [("me", "Marie, tu peux corriger le APP-1234 d'ici vendredi ?"),
           ("them", "Oui, je m'en occupe, je corrige le APP-1234 pour vendredi.")],
    1700: [("them", "Au fait, l'outil Sténo dont tu parlais, tu l'utilises pour quoi exactement ?"),
           ("me", "Pour prendre des notes, comme une sténographie, c'est devenu une de mes habitudes."),
           ("me", "On a vu le steno hier, cherche pas plus loin.")],
    2000: [("me", "Sténo, note que Marie relance le support Brindille."),
           ("them", "Je relancerai le support Brindille cet après-midi.")],
    2600: [("them", "Bon, je crois qu'on a fait le tour."), ("me", "Merci à tous, bonne journée !"),
           ("them", "Merci, à demain.")],
}

FILLER_THEM = [
    "Sur le front j'ai fini la page de récapitulatif de la location.",
    "La revue de code de la MR 482 est en cours, il reste deux commentaires.",
    "On a un souci de quota sur l'environnement de recette, je regarde avec l'infra.",
    "Les tests end-to-end sont au vert depuis ce matin.",
    "Je vais pairer avec Paul sur la reprise des données clients.",
    "Le monitor de latence sur ms-booking a sonné deux fois cette semaine.",
    "J'ai ajouté des logs autour de la saga de création de compte.",
    "Pour le sprint prochain on a encore trois stories à estimer.",
    "La démo de jeudi est confirmée avec le métier.",
    "Le déploiement en préproduction est prévu cet après-midi.",
    "Je n'ai pas de point bloquant aujourd'hui.",
    "On a un ticket support sur les factures de location en PDF, je l'ai pris.",
    "J'ai mis à jour la documentation de l'API des stations.",
    "Le batch de nuit a mis quarante minutes de plus que d'habitude.",
    "Il faudrait qu'on parle de la dette technique sur le module de tarification.",
]
FILLER_ME = [
    "D'accord, merci.",
    "Ok, tu me tiens au courant.",
    "Je regarde ça après la réunion.",
    "Est-ce que quelqu'un a besoin d'aide là-dessus ?",
    "Ça me paraît bien.",
    "On en reparle en fin de semaine.",
    "Tu peux mettre le lien dans le canal ?",
    "Très bien, on avance.",
]

t = 0.0
slots = sorted(SCRIPTED)
events.append({"type": "teams_call", "active": True, "title": "Daily ORION"})
events.append({"type": "state", "state": "listening"})
for slot in slots + [DURATION]:
    # Filler up to the next required moment.
    while t + 12 < slot:
        t += random.uniform(0.8, 2.5)
        if random.random() < 0.7:
            t = seg(t, "them", random.choice(FILLER_THEM))
        else:
            t = seg(t, "me", random.choice(FILLER_ME))
    if slot == DURATION:
        break
    t = max(t, slot)
    for ch, text in SCRIPTED[slot]:
        if ch == "me@echo":
            # Echo: the microphone picks up the speakers 0.4 s after the Teams channel.
            last = events[-1]
            seg(last["t0"] + 0.4, "me", text, dur=last["t1"] - last["t0"])
            continue
        private = ch == "me@private"
        t += random.uniform(0.6, 1.2)
        t = seg(t, "me" if ch.startswith("me") else "them", text, private=private)
events.append({"type": "teams_call", "active": False, "title": None})

out = Path(__file__).with_name("daily-45min.jsonl")
out.write_text("\n".join(json.dumps(e, ensure_ascii=False) for e in events) + "\n", encoding="utf-8")

expected = {
    "title": "Daily ORION",
    "voice_commands": [
        {"intent": "search", "query_contains": "erreurs Datadog sur ms-booking"},
        {"intent": "note", "query_contains": "Marie relance le support Brindille"},
    ],
    "private_segments": 2,
    "echo_segments": 1,
    "decision_contains": "verrou de créneau",
    "actions": [
        {"owner": "Marie", "text_contains": "APP-1234"},
        {"owner": "Moi", "text_contains": "Confluence"},
    ],
    "segments": sum(1 for e in events if e["type"] == "segment"),
}
out.with_name("expected.json").write_text(json.dumps(expected, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
print(f"{out.name}: {expected['segments']} segments, {t / 60:.1f} min")
