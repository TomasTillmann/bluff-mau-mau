"""Small comparable player policies."""

from .random_legal import RandomLegal
from .honest_first import HonestFirst

__all__ = ["RandomLegal", "HonestFirst"]
