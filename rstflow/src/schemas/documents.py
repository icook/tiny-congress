"""Schemas for raw document records."""

from pydantic import BaseModel, Field


class RawDocument(BaseModel):
    """Raw civic-discourse document."""

    doc_id: str = Field(..., description="Unique identifier for the document.")
    topic_id: str = Field(..., description="Topic grouping identifier.")
    author_id: str = Field(..., description="Synthetic author identifier.")
    text: str = Field(..., description="Full document text.")
