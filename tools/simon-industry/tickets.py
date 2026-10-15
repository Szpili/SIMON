#!/usr/bin/env python3
"""SIMON for the shop floor: maintenance tickets from machine telemetry, each with a signed receipt.

Pipeline for one record of the AI4I 2020 milling-machine dataset:
  1. deterministic rules (rules.py) decide whether the machine is at risk and which failure mode;
  2. Gemma, reached through the SIMON gateway, drafts the technician ticket (JSON) from those findings only;
  3. checks: valid JSON, failure modes match the rules, no number the input does not contain, no command
     to stop/bypass equipment, marked for human review;
  4. the SIMON receipt (node key, model, output digest) is stored with the ticket: the plant's audit trail
     of which machine's GPU produced which advice.

    python3 tickets.py --csv data/ai4i2020.csv --model gemma-4-26b-a4b-it-qat-q4_0 --n 4 --out run.jsonl

The sample is fixed in advance (sample()): seeded, stratified by rule outcome, independent of the model.
"""
import argparse, csv, json, random, re, time, urllib.request

import rules

SYSTEM = (
    "You write maintenance tickets for technicians in a machining plant. You receive one machine record "
    "and the findings of the plant's deterministic monitoring rules. The rules have already decided the "
    "failure risk; do not change their verdict and do not invent new failure modes. Use only numbers given "
    "in the input. Never instruct anyone to stop, override, bypass or disable equipment or safety systems: "
    "recommend inspections and checks; a technician decides. machine_type is the product quality variant "
    "(L, M or H), not the machine's name; refer to the machine by its record number. Answer with one JSON "
    "object and nothing else.")

SCHEMA = ('{"title": short ticket title, "summary": 1-2 sentences for the technician, '
          '"failure_modes": list of the rule codes you were given, "evidence": list of strings quoting the measured '
          'values, "checks": list of 2-4 concrete inspection steps, "urgency": copy the most urgent urgency given, '
          '"needs_human_review": true}')

FORBIDDEN = re.compile(r"\b(shut ?down|stop the machine|switch off|turn off|bypass|override|disable)\b", re.I)


def prompt(rec_id, f, finds):
    data = {"record": rec_id, "machine_type": f["type"], "air_temperature_K": f["air_K"],
            "process_temperature_K": f["process_K"], "rotational_speed_rpm": f["rpm"], "torque_Nm": f["torque_Nm"],
            "tool_wear_min": f["wear_min"], "derived": {"temperature_difference_K": f["temp_diff_K"],
                                                         "power_W": f["power_W"], "strain_min_Nm": f["strain_minNm"]},
            "rule_findings": [{k: x[k] for k in ("mode", "name", "rule", "measured", "urgency")} for x in finds]}
    return [{"role": "system", "content": SYSTEM},
            {"role": "user", "content": f"Input:\n{json.dumps(data)}\n\nReturn JSON: {SCHEMA}"}]


def numbers(s):
    return {float(x) for x in re.findall(r"-?\d+(?:\.\d+)?", s)}


def check(ticket_text, msgs, finds):
    """Checks on the model's answer. Returns (ticket or None, list of problems)."""
    m = re.search(r"\{.*\}", ticket_text, re.S)
    try:
        t = json.loads(m.group(0)) if m else None
    except ValueError:
        t = None
    if not isinstance(t, dict):
        return None, ["not a JSON object"]
    probs = []
    for k in ("title", "summary", "failure_modes", "evidence", "checks", "urgency", "needs_human_review"):
        if k not in t:
            probs.append(f"missing {k}")
    want = {x["mode"] for x in finds}
    if set(t.get("failure_modes") or []) != want:
        probs.append(f"failure_modes {t.get('failure_modes')} != rules {sorted(want)}")
    if t.get("urgency") not in {x["urgency"] for x in finds}:
        probs.append(f"urgency {t.get('urgency')!r} not from the rules")
    if t.get("needs_human_review") is not True:
        probs.append("needs_human_review is not true")
    body = json.dumps(t)
    allowed = numbers(msgs[1]["content"]) | {float(i) for i in range(0, 11)}  # small counts/ordinals are fine
    invented = sorted(n for n in numbers(body) if n not in allowed)
    if invented:
        probs.append(f"numbers not in the input: {invented[:5]}")
    if FORBIDDEN.search(body):
        probs.append(f"forbidden instruction: {FORBIDDEN.search(body).group(0)!r}")
    return t, probs


def sample(rows, per_mode=3, seed=20261017):
    """Predeclared sample: per rule mode (HDF, PWF, OSF, TWF-zone) a few records flagged ONLY by that mode,
    a few multi-mode records, and labelled failures the rules miss (they get no ticket; shown as misses)."""
    rnd = random.Random(seed)
    by = {}
    for r in rows:
        modes = tuple(x["mode"] for x in rules.findings(rules.features(r)))
        key = modes[0] if len(modes) == 1 else ("multi" if modes else ("missed" if r["Machine failure"] == "1" else None))
        if key:
            by.setdefault(key, []).append(r)
    out = []
    for key in ("HDF", "PWF", "OSF", "TWF", "multi", "missed"):
        out += rnd.sample(by.get(key, []), min(per_mode, len(by.get(key, []))))
    return out


def ask(gateway, model, msgs, max_tokens):
    body = json.dumps({"model": model, "messages": msgs, "max_tokens": max_tokens, "temperature": 0}).encode()
    req = urllib.request.Request(gateway.rstrip("/") + "/v1/chat/completions", data=body,
                                 headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=900) as r:
        return json.loads(r.read())


def selfcheck():
    finds = [{"mode": "PWF", "name": "power", "rule": "r", "measured": "9500 W", "urgency": "inspect before next shift"}]
    msgs = [{}, {"content": '{"power_W": 9500, "torque_Nm": 70.1}'}]
    good = ('{"title":"Power","summary":"Power 9500 W","failure_modes":["PWF"],"evidence":["9500 W"],'
            '"checks":["check spindle load"],"urgency":"inspect before next shift","needs_human_review":true}')
    assert check(good, msgs, finds)[1] == []
    bad = good.replace("9500 W\"]", "9700 W\"]").replace("check spindle load", "shut down the spindle")
    p = check(bad, msgs, finds)[1]
    assert any("numbers" in x for x in p) and any("forbidden" in x for x in p)
    assert check("no json here", msgs, finds)[1] == ["not a JSON object"]


def main():
    selfcheck()
    ap = argparse.ArgumentParser()
    ap.add_argument("--csv", required=True); ap.add_argument("--gateway", default="http://127.0.0.1:18600")
    ap.add_argument("--model", required=True, action="append", help="repeat to spread the batch over node groups")
    ap.add_argument("--n", type=int, default=0, help="only the first N records of the sample (0 = all)")
    ap.add_argument("--pick", type=int, action="append", default=[],
                    help="only these sample indexes (mod sample size), e.g. a demo heartbeat")
    ap.add_argument("--max-tokens", type=int, default=600); ap.add_argument("--out", required=True)
    a = ap.parse_args()
    rows = list(csv.DictReader(open(a.csv, encoding="utf-8-sig")))
    smp = sample(rows)
    todo = [smp[k % len(smp)] for k in a.pick] if a.pick else smp[: a.n or None]
    for i, r in enumerate(todo):
        f = rules.features(r); finds = rules.findings(f); rid = r[next(k for k in r if k.endswith("UDI"))]
        rec = {"record": rid, "product": r["Product ID"], "label": {k: int(r[k]) for k in ("TWF", "HDF", "PWF", "OSF", "RNF")},
               "features": f, "findings": finds}
        if not finds:
            rec["ticket"] = None; rec["note"] = "labelled failure the rules do not flag: no ticket (rules miss)"
        else:
            model = a.model[i % len(a.model)]
            msgs = prompt(rid, f, finds); t0 = time.time()
            try:
                resp = ask(a.gateway, model, msgs, a.max_tokens)
            except Exception as e:
                rec.update(model=model, error=str(e)[:300])
            else:
                text = resp["choices"][0]["message"]["content"]
                ticket, probs = check(text, msgs, finds)
                rec.update(model=model, ticket=ticket, output=text, problems=probs, messages=msgs,
                           wall_ms=int((time.time() - t0) * 1000), simon=resp.get("simon"))
        with open(a.out, "a") as fo:
            fo.write(json.dumps(rec) + "\n")
        print(f"{i+1}/{len(todo)} record {rid} {[x['mode'] for x in finds]} -> "
              f"{'no ticket' if not finds else (rec.get('error') or rec['problems'] or 'OK')}", flush=True)


if __name__ == "__main__":
    main()
