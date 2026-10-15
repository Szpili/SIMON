"""Deterministic failure-mode rules for the AI4I 2020 milling machine (the dataset's own published physics).

The language model never decides whether a machine is at risk: these rules do. Gemma only writes the
maintenance ticket for a technician from the rule's findings.

Source of the thresholds: the AI4I 2020 dataset description (UCI, CC BY 4.0).
"""
import math

OSF_LIMIT = {"L": 11000, "M": 12000, "H": 13000}  # tool wear [min] x torque [Nm], by product quality type


# The only inspection steps a ticket may contain, per failure mode. The model picks IDs; it never writes
# the step itself, so it cannot author an instruction for the technician (stress test, Joe, 2026-10-15).
CHECKS = {
    "HDF": {"HDF-1": "Check the cooling air flow at the spindle housing (fans, filters, ducts)",
            "HDF-2": "Inspect cooling fins and heat exchanger surfaces for dust or blockage",
            "HDF-3": "Compare the air and process temperature sensors against a reference thermometer"},
    "PWF": {"PWF-1": "Check spindle motor current and torque against the drive's rated values",
            "PWF-2": "Inspect the cutting tool and workpiece clamping for binding or slip",
            "PWF-3": "Review the cutting parameters (speed, feed) of the current job with the process engineer"},
    "OSF": {"OSF-1": "Measure the cutting tool's wear and compare it with the replacement criterion",
            "OSF-2": "Review the torque load of the last jobs for overload",
            "OSF-3": "Inspect the tool holder and spindle bearings for play or vibration"},
    "TWF": {"TWF-1": "Measure the tool's flank wear and plan the tool change",
            "TWF-2": "Confirm a replacement tool of the right type is in stock",
            "TWF-3": "Record the tool's run time in the tool log"},
}


def features(r):
    air, proc = float(r["Air temperature [K]"]), float(r["Process temperature [K]"])
    rpm, torque, wear = float(r["Rotational speed [rpm]"]), float(r["Torque [Nm]"]), float(r["Tool wear [min]"])
    return {"type": r["Type"], "air_K": air, "process_K": proc, "rpm": rpm, "torque_Nm": torque, "wear_min": wear,
            "temp_diff_K": round(proc - air, 1), "power_W": round(torque * rpm * 2 * math.pi / 60),
            "strain_minNm": round(wear * torque)}


def findings(f):
    """Rule hits for one record: [{mode, rule, measured, limit, urgency}], most urgent first."""
    out = []
    if f["temp_diff_K"] < 8.6 and f["rpm"] < 1380:
        out.append({"mode": "HDF", "name": "heat dissipation",
                    "rule": "process - air temperature < 8.6 K and speed < 1380 rpm",
                    "measured": f"{f['temp_diff_K']} K at {f['rpm']:.0f} rpm", "urgency": "inspect before next shift"})
    if not 3500 <= f["power_W"] <= 9000:
        out.append({"mode": "PWF", "name": "power", "rule": "power outside 3500-9000 W",
                    "measured": f"{f['power_W']} W", "urgency": "inspect before next shift"})
    lim = OSF_LIMIT[f["type"]]
    if f["strain_minNm"] > lim:
        out.append({"mode": "OSF", "name": "overstrain", "rule": f"tool wear x torque > {lim} min*Nm (type {f['type']})",
                    "measured": f"{f['strain_minNm']} min*Nm", "urgency": "inspect before next shift"})
    if f["wear_min"] >= 200:
        out.append({"mode": "TWF", "name": "tool wear", "rule": "tool wear >= 200 min (failure zone 200-240 min)",
                    "measured": f"{f['wear_min']:.0f} min", "urgency": "plan tool change"})
    return out


def selfcheck():
    r = {"Type": "L", "Air temperature [K]": "300.0", "Process temperature [K]": "308.0",
         "Rotational speed [rpm]": "1300", "Torque [Nm]": "60", "Tool wear [min]": "210"}
    f = features(r)
    assert f["power_W"] == round(60 * 1300 * 2 * math.pi / 60) and f["temp_diff_K"] == 8.0
    assert [x["mode"] for x in findings(f)] == ["HDF", "OSF", "TWF"]  # 8170 W is inside the power band
    assert all(k.startswith(m + "-") for m, c in CHECKS.items() for k in c)


if __name__ == "__main__":
    selfcheck()
    import csv, sys
    if len(sys.argv) > 1:
        rows = list(csv.DictReader(open(sys.argv[1], encoding="utf-8-sig")))
        for mode in ("TWF", "HDF", "PWF", "OSF", "RNF"):
            lab = [r for r in rows if r[mode] == "1"]
            hit = [r for r in lab if mode in [x["mode"] for x in findings(features(r))]]
            flagged = [r for r in rows if mode in [x["mode"] for x in findings(features(r))]]
            print(f"{mode}: labelled {len(lab)}, rule recall {len(hit)}/{len(lab)}, rule fires on {len(flagged)} records")
        anyflag = [r for r in rows if findings(features(r))]
        fail = [r for r in rows if r["Machine failure"] == "1"]
        print(f"any rule fires: {len(anyflag)} records; machine failures caught: "
              f"{sum(1 for r in fail if findings(features(r)))}/{len(fail)}")
