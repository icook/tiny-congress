"""Schemas for RST parsing results."""

from __future__ import annotations

from typing import Literal, Optional, Tuple

from pydantic import BaseModel, Field


class RSTEDU(BaseModel):
    """Elementary discourse unit produced by an RST parser."""

    edu_id: str = Field(..., description="Unique identifier for the EDU within the document.")
    text: str = Field(..., description="Surface text of the EDU.")
    span: Optional[Tuple[int, int]] = Field(
        default=None,
        description="Optional character span (start, end) within the original document.",
    )


class RSTRelation(BaseModel):
    """Parent-child relationship between EDUs."""

    child_id: str = Field(..., description="Identifier of the child EDU.")
    parent_id: Optional[str] = Field(
        default=None, description="Identifier of the parent EDU; None when child is the root."
    )
    relation: str = Field(..., description="Relation label assigned by the parser.")
    nuclearity: Literal["nucleus", "satellite"] = Field(
        ..., description="Nuclearity of the child with respect to its parent."
    )


class RSTParseResult(BaseModel):
    """Normalised parse output consumed by the pipeline."""

    edus: list[RSTEDU] = Field(..., description="All EDUs in document order.")
    relations: list[RSTRelation] = Field(
        ..., description="Relations mapping each EDU to its parent and nuclearity."
    )
    root_edu: Optional[str] = Field(
        default=None,
        description="Identifier of the root EDU, if known.",
    )
