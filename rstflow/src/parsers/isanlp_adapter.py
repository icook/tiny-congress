"""Adapter for the IsaNLP RST parser."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Optional


class IsaNLPNotInstalledError(ImportError):
    """Raised when isanlp-rst is not available in the environment."""


class IsaNLPRuntimeError(RuntimeError):
    """Raised when IsaNLP returns an unexpected payload."""


@dataclass
class IsaNLPParserAdapter:
    """Shim that normalises IsaNLP output to RSTFlow's schema."""

    model_name: str = "tchewik/isanlp_rst_v3"
    model_version: str = "rstdt"
    cuda_device: int = -1
    relinventory: Optional[str] = None

    def __post_init__(self) -> None:
        try:
            from isanlp_rst.parser import Parser as IsaNLPParser  # type: ignore
        except ImportError as exc:  # pragma: no cover - depends on optional dep
            raise IsaNLPNotInstalledError(
                "isanlp-rst is not installed. Install it via 'pip install isanlp-rst' to use the IsaNLP backend."
            ) from exc

        kwargs: dict[str, Any] = {
            "hf_model_name": self.model_name,
            "hf_model_version": self.model_version,
            "cuda_device": self.cuda_device,
        }
        if self.relinventory:
            kwargs["relinventory"] = self.relinventory

        self._parser = IsaNLPParser(**kwargs)
        self.name = "isanlp"
        self.version = f"{self.model_name}:{self.model_version}"

    def parse(self, text: str) -> dict[str, Any]:
        parsed = self._parser(text)
        rst_root = parsed.get("rst") if isinstance(parsed, dict) else None
        if not rst_root:
            raise IsaNLPRuntimeError("IsaNLP response missing 'rst' field.")

        if isinstance(rst_root, list):
            tree = rst_root[0]
        else:
            tree = rst_root

        if tree is None:
            raise IsaNLPRuntimeError("IsaNLP returned an empty tree.")

        edus: list[dict[str, Any]] = []
        relations: list[dict[str, Any]] = []

        primary_nucleus: dict[str, Optional[str]] = {"id": None}

        def _attr(node: Any, name: str, default: Any = None) -> Any:
            if hasattr(node, name):
                return getattr(node, name)
            if isinstance(node, dict):
                return node.get(name, default)
            return default

        def _decode_nuclearity(code: Optional[str]) -> tuple[str, str]:
            if not code:
                return "nucleus", "nucleus"
            norm = code.upper()
            if norm in {"NS", "SN", "NN"}:
                mapping = {
                    "N": "nucleus",
                    "S": "satellite",
                }
                return mapping.get(norm[0], "nucleus"), mapping.get(norm[1], "nucleus")
            if norm in {"N", "S"}:
                status = "nucleus" if norm == "N" else "satellite"
                return status, status
            parts = norm.split("-")
            if len(parts) == 2:
                mapping = {
                    "N": "nucleus",
                    "S": "satellite",
                }
                return mapping.get(parts[0][:1], "nucleus"), mapping.get(parts[1][:1], "nucleus")
            return "nucleus", "nucleus"

        def _collect(node: Any, parent_id: Optional[str] = None, relation_label: Optional[str] = None, child_role: Optional[str] = None) -> None:
            node_id = _attr(node, "id")
            if node_id is None:
                raise IsaNLPRuntimeError("IsaNLP node missing id")
            node_id = str(node_id)
            text_span = _attr(node, "text", "")
            start = _attr(node, "start")
            end = _attr(node, "end")
            left = _attr(node, "left")
            right = _attr(node, "right")

            if parent_id is not None:
                relations.append(
                    {
                        "child_id": node_id,
                        "parent_id": parent_id,
                        "relation": relation_label,
                        "nuclearity": child_role or "nucleus",
                    }
                )

            if left is None and right is None:
                span = None
                if isinstance(start, (int, float)) and isinstance(end, (int, float)):
                    span = (int(start), int(end))
                edus.append(
                    {
                        "edu_id": node_id,
                        "text": text_span.strip() if isinstance(text_span, str) else "",
                        "span": span,
                    }
                )
                return

            relation = _attr(node, "relation", relation_label)
            nuclearity = _attr(node, "nuclearity")
            left_role, right_role = _decode_nuclearity(nuclearity)

            if parent_id is None:
                primary_nucleus["id"] = node_id

            if left is not None:
                _collect(left, parent_id=node_id, relation_label=relation, child_role=left_role)
            if right is not None:
                _collect(right, parent_id=node_id, relation_label=relation, child_role=right_role)

        _collect(tree)

        if not edus:
            raise IsaNLPRuntimeError("IsaNLP produced no EDUs.")

        def _find_primary_leaf(node: Any) -> str:
            left = _attr(node, "left")
            right = _attr(node, "right")
            if left is None and right is None:
                node_id = _attr(node, "id")
                return str(node_id)
            relation = _attr(node, "relation")
            nuclearity = _attr(node, "nuclearity")
            left_role, right_role = _decode_nuclearity(nuclearity)
            if left_role == "nucleus" and left is not None:
                return _find_primary_leaf(left)
            if right_role == "nucleus" and right is not None:
                return _find_primary_leaf(right)
            if left is not None:
                return _find_primary_leaf(left)
            if right is not None:
                return _find_primary_leaf(right)
            node_id = _attr(node, "id")
            return str(node_id)

        root_leaf_id = _find_primary_leaf(tree)

        return {
            "edus": edus,
            "relations": relations,
            "root_edu": root_leaf_id,
        }
