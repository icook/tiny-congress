#!/usr/bin/env python3
"""Generate Grafana dashboard JSON from Foundation SDK definitions.

Usage:
    python kube/dashboards/generate.py            # generate + validate
    python kube/dashboards/generate.py --check     # validate only (no write)

Output is written to kube/app/files/dashboards/ so Helm's .Files.Get can reach it.
"""

import argparse
import json
import sys
from pathlib import Path

from grafana_foundation_sdk.cog.encoder import JSONEncoder

import health

CHART_DIR = Path(__file__).parent.parent / "app"
OUTPUT_DIR = CHART_DIR / "files" / "dashboards"


def validate(data: dict) -> list[str]:
    """Return a list of validation errors (empty = pass)."""
    errors = []

    if data.get("title") != "Tiny Congress Health":
        errors.append(f"unexpected title: {data.get('title')}")

    elements = data.get("elements", {})
    if len(elements) < 10:
        errors.append(f"expected >=10 elements, got {len(elements)}")

    layout = data.get("layout", {})
    if layout.get("kind") != "RowsLayout":
        errors.append(f"expected RowsLayout, got {layout.get('kind')}")
    else:
        rows = layout["spec"].get("rows", [])
        if len(rows) != 5:
            errors.append(f"expected 5 rows, got {len(rows)}")

    variables = data.get("variables", [])
    var_names = {v["spec"]["name"] for v in variables}
    for required in ("namespace", "prometheus_datasource", "loki_datasource"):
        if required not in var_names:
            errors.append(f"missing variable: {required}")

    # Every element referenced in a grid item must exist
    for row in layout.get("spec", {}).get("rows", []):
        for item in row.get("spec", {}).get("layout", {}).get("spec", {}).get("items", []):
            ref = item.get("spec", {}).get("element", {}).get("spec", {}).get("name", "")
            if ref and ref not in elements:
                errors.append(f"grid references missing element: {ref}")

    return errors


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true", help="validate without writing")
    args = parser.parse_args()

    dashboard = health.build()
    built = dashboard.build()
    rendered = json.loads(json.dumps(built, cls=JSONEncoder))

    errors = validate(rendered)
    if errors:
        for e in errors:
            print(f"FAIL: {e}", file=sys.stderr)
        sys.exit(1)

    print(f"OK: {len(rendered['elements'])} panels, {len(rendered['layout']['spec']['rows'])} rows")

    if not args.check:
        OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
        out_path = OUTPUT_DIR / "health.json"
        out_path.write_text(
            json.dumps(built, cls=JSONEncoder, indent=2, sort_keys=False) + "\n"
        )
        print(f"Wrote {out_path}")


if __name__ == "__main__":
    main()
