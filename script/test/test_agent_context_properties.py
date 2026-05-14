# -*- coding: utf-8 -*-
"""Property-based tests for context file truncation.

Property 18: Context file truncation.
Validates: Requirements 11.5

For any user context file content exceeding 32768 characters, the loaded content
SHALL be exactly 32768 characters (truncated from the original), and a warning
SHALL be logged.
"""

from __future__ import annotations

import logging
import logging.handlers
import tempfile
from pathlib import Path

from hypothesis import HealthCheck, given, settings
from hypothesis import strategies as st

from src.core.agent.context_loader import MAX_CONTEXT_LENGTH, ContextLoader


# ============================================================
# Strategies
# ============================================================


@st.composite
def oversized_content(draw: st.DrawFn) -> str:
    """Generate strings that are strictly longer than MAX_CONTEXT_LENGTH.

    Hypothesis has internal limits on min_size for st.text(), so we generate
    a base string and repeat it to exceed the threshold, then append a random
    suffix to ensure variability in the tail portion.
    """
    # Generate a base chunk (1000-5000 chars) and repeat to exceed limit
    base = draw(st.text(
        min_size=1000,
        max_size=5000,
        alphabet=st.characters(categories=("L", "N", "P", "Z", "S")),
    ))
    # Calculate how many repetitions we need to exceed MAX_CONTEXT_LENGTH
    reps = (MAX_CONTEXT_LENGTH // len(base)) + 2
    long_content = base * reps

    # Add a random extra suffix (1 to 1000 chars) for variability
    extra = draw(st.integers(min_value=1, max_value=1000))
    suffix = draw(st.text(
        min_size=extra,
        max_size=extra,
        alphabet=st.characters(categories=("L", "N", "P", "Z", "S")),
    ))
    result = long_content + suffix

    # Ensure it's strictly longer than MAX_CONTEXT_LENGTH
    assert len(result) > MAX_CONTEXT_LENGTH
    return result


_oversized_content_strategy = oversized_content()


# ============================================================
# Property Tests
# ============================================================


class TestContextFileTruncationProperty:
    """Property 18: Context file truncation.

    **Validates: Requirements 11.5**
    """

    @settings(max_examples=100, suppress_health_check=[HealthCheck.too_slow])
    @given(content=_oversized_content_strategy)
    def test_truncated_content_is_exactly_max_length(
        self,
        content: str,
    ) -> None:
        """For any user context file content exceeding 32768 characters,
        the loaded content SHALL be exactly 32768 characters.

        **Validates: Requirements 11.5**
        """
        with tempfile.TemporaryDirectory() as tmp_dir:
            context_file = Path(tmp_dir) / "agent_context.md"
            context_file.write_text(content, encoding="utf-8")

            loader = ContextLoader(context_file)
            result = loader.load()

            assert len(result) == MAX_CONTEXT_LENGTH, (
                f"Expected loaded content length to be exactly {MAX_CONTEXT_LENGTH}, "
                f"but got {len(result)} (original was {len(content)} chars)"
            )

    @settings(max_examples=100, suppress_health_check=[HealthCheck.too_slow])
    @given(content=_oversized_content_strategy)
    def test_truncated_content_equals_first_max_length_chars(
        self,
        content: str,
    ) -> None:
        """For any user context file content exceeding 32768 characters,
        the loaded content SHALL equal the first 32768 characters of the original.

        **Validates: Requirements 11.5**
        """
        with tempfile.TemporaryDirectory() as tmp_dir:
            context_file = Path(tmp_dir) / "agent_context.md"
            context_file.write_text(content, encoding="utf-8")

            loader = ContextLoader(context_file)
            result = loader.load()

            expected = content[:MAX_CONTEXT_LENGTH]
            assert result == expected, (
                f"Truncated content does not match the first {MAX_CONTEXT_LENGTH} "
                f"characters of the original content."
            )

    @settings(max_examples=100, suppress_health_check=[HealthCheck.too_slow])
    @given(content=_oversized_content_strategy)
    def test_warning_logged_on_truncation(
        self,
        content: str,
    ) -> None:
        """For any user context file content exceeding 32768 characters,
        a warning SHALL be logged.

        **Validates: Requirements 11.5**
        """
        with tempfile.TemporaryDirectory() as tmp_dir:
            context_file = Path(tmp_dir) / "agent_context.md"
            context_file.write_text(content, encoding="utf-8")

            loader = ContextLoader(context_file)

            # Use a logging handler to capture log records within hypothesis
            logger = logging.getLogger("src.core.agent.context_loader")
            handler = logging.handlers.MemoryHandler(capacity=100)
            handler.setLevel(logging.WARNING)
            logger.addHandler(handler)
            try:
                loader.load()
                handler.flush()

                # Check that at least one warning was emitted
                warning_records = [
                    r for r in handler.buffer if r.levelno == logging.WARNING
                ]
                assert len(warning_records) >= 1, (
                    "No warning was logged when content exceeded MAX_CONTEXT_LENGTH."
                )

                # The warning message should mention the character limit
                warning_msg = warning_records[0].getMessage()
                assert str(MAX_CONTEXT_LENGTH) in warning_msg, (
                    f"Warning message does not mention the limit "
                    f"({MAX_CONTEXT_LENGTH}). Got: {warning_msg!r}"
                )
            finally:
                logger.removeHandler(handler)
