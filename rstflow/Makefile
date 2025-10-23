VENV ?= .venv
PYTHON ?= python3
PIP ?= $(VENV)/bin/pip
PYTEST ?= $(VENV)/bin/pytest
COUNT ?= 100
OUTPUT ?= data/raw/docs.jsonl

.PHONY: venv install test synth clean

venv:
	$(PYTHON) -m venv $(VENV)
	$(VENV)/bin/pip install --upgrade pip

install: venv
	$(PIP) install -r requirements.txt

test: install
	$(PYTEST) tests

synth: install
	$(VENV)/bin/python src/cli.py --count $(COUNT) --output $(OUTPUT)

clean:
	rm -rf $(VENV)
	find . -name "__pycache__" -type d -prune -exec rm -rf {} +
