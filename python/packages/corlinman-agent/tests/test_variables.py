"""Tests for the variable cascade.

The cascade is a sync API backed by an async hot-reload watcher. Tests
exercise each tier in isolation, then verify that filesystem mutations
propagate through the watcher within its polling window.
"""

from __future__ import annotations

import asyncio
import os
import re
from datetime import datetime
from pathlib import Path

import pytest
from corlinman_agent.variables import VariableCascade
from corlinman_agent.variables.hot_reload import HotReloadWatcher


def _make_cascade(
    tmp_path: Path,
    *,
    with_tar: bool = True,
    with_var: bool = True,
    with_sar: bool = True,
    hot_reload: bool = False,
) -> VariableCascade:
    """Assemble a cascade against freshly-created TVStxt-style subdirs."""
    tar = tmp_path / "tar" if with_tar else None
    var = tmp_path / "var" if with_var else None
    sar = tmp_path / "sar" if with_sar else None
    for d in (tar, var, sar):
        if d is not None:
            d.mkdir(parents=True, exist_ok=True)
    return VariableCascade(tar, var, sar, None, hot_reload=hot_reload)


def test_fixed_timevar_is_iso8601(tmp_path: Path) -> None:
    cascade = _make_cascade(tmp_path)
    value = cascade.resolve("TimeVar", model_name="anything")
    assert value is not None
    # ``datetime.fromisoformat`` accepts the ``Z`` suffix only on 3.11+,
    # but this package already pins >=3.12 so the round-trip is safe.
    parsed = datetime.fromisoformat(value.replace("Z", "+00:00"))
    assert parsed.tzinfo is not None


def test_fixed_date_format(tmp_path: Path) -> None:
    cascade = _make_cascade(tmp_path)
    value = cascade.resolve("Date", model_name="anything")
    assert value is not None
    assert re.fullmatch(r"\d{4}-\d{2}-\d{2}", value)


def test_tar_reads_file_by_key(tmp_path: Path) -> None:
    cascade = _make_cascade(tmp_path)
    (tmp_path / "tar" / "ModelName.txt").write_text("claude-3-opus\n", encoding="utf-8")
    assert cascade.resolve("TarModelName", model_name="irrelevant") == "claude-3-opus"


def test_var_env_passthrough_when_no_matching_file(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    cascade = _make_cascade(tmp_path)
    monkeypatch.setenv("VarUserName", "ian")
    # No ``TVStxt/var/ian.txt`` → env value is the value.
    assert cascade.resolve("VarUserName", model_name="anything") == "ian"


def test_var_env_triggers_file_load_when_match(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    cascade = _make_cascade(tmp_path)
    (tmp_path / "var" / "persona_pirate.txt").write_text(
        "You speak like a pirate.\n", encoding="utf-8"
    )
    monkeypatch.setenv("VarPersona", "persona_pirate")
    assert (
        cascade.resolve("VarPersona", model_name="anything")
        == "You speak like a pirate."
    )


def test_sar_model_match_loads_file(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    cascade = _make_cascade(tmp_path)
    (tmp_path / "sar" / "SarPrompt4.txt").write_text(
        "Opus-only instructions.\n", encoding="utf-8"
    )
    monkeypatch.setenv("SarModel4", "claude-3-opus,gpt-4")
    assert (
        cascade.resolve("SarPrompt4", model_name="claude-3-opus")
        == "Opus-only instructions."
    )


def test_sar_model_match_case_insensitive(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    cascade = _make_cascade(tmp_path)
    (tmp_path / "sar" / "SarPrompt4.txt").write_text("x", encoding="utf-8")
    monkeypatch.setenv("SarModel4", "Claude-3-Opus,GPT-4")
    assert cascade.resolve("SarPrompt4", model_name="claude-3-opus") == "x"


def test_sar_model_miss_returns_empty_string(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    cascade = _make_cascade(tmp_path)
    (tmp_path / "sar" / "SarPrompt4.txt").write_text("secret", encoding="utf-8")
    monkeypatch.setenv("SarModel4", "claude-3-opus")
    # Gate set but our model isn't in the list → empty string, NOT None
    # (so the placeholder engine drops the token cleanly).
    assert cascade.resolve("SarPrompt4", model_name="gpt-4") == ""


def test_sar_unset_env_returns_empty_string(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    cascade = _make_cascade(tmp_path)
    (tmp_path / "sar" / "SarPrompt9.txt").write_text("x", encoding="utf-8")
    monkeypatch.delenv("SarModel9", raising=False)
    assert cascade.resolve("SarPrompt9", model_name="claude-3-opus") == ""


def test_resolve_unknown_key_returns_none(tmp_path: Path) -> None:
    cascade = _make_cascade(tmp_path)
    assert cascade.resolve("CompletelyMadeUpKey", model_name="anything") is None


def test_fixed_overrides_tar(tmp_path: Path) -> None:
    """Fixed tier wins even if a same-named tar file exists."""
    cascade = _make_cascade(tmp_path)
    (tmp_path / "tar" / "Something.txt").write_text("from-disk", encoding="utf-8")
    cascade.register_fixed("TarSomething", lambda: "from-fixed")
    assert cascade.resolve("TarSomething", model_name="x") == "from-fixed"


async def test_hot_reload_invalidates_cache(tmp_path: Path) -> None:
    """Write file, read, mutate, read again — second read must see the
    new content within one polling window (<1.5s)."""
    cascade = _make_cascade(tmp_path, hot_reload=True)
    target = tmp_path / "tar" / "Rolling.txt"
    target.write_text("one", encoding="utf-8")

    # Prime the cache before starting the watcher so the watcher's
    # initial snapshot captures the original mtime.
    assert cascade.resolve("TarRolling", model_name="x") == "one"
    await cascade.start_watching()
    try:
        # Use a tight poll interval for the test; swap the watcher
        # interval via its private attribute — public API is 1s.
        watcher = cascade._watcher
        assert watcher is not None
        watcher._interval = 0.1

        # Bump mtime explicitly to clear any fs granularity ambiguity
        # (HFS+ / some CI runners truncate to 1s).
        target.write_text("two", encoding="utf-8")
        new_mtime = target.stat().st_mtime + 2.0
        os.utime(target, (new_mtime, new_mtime))

        deadline = asyncio.get_event_loop().time() + 1.5
        while asyncio.get_event_loop().time() < deadline:
            val = cascade.resolve("TarRolling", model_name="x")
            if val == "two":
                break
            await asyncio.sleep(0.05)
        assert cascade.resolve("TarRolling", model_name="x") == "two"
    finally:
        await cascade.stop_watching()


async def test_hot_reload_stop_is_idempotent(tmp_path: Path) -> None:
    cascade = _make_cascade(tmp_path, hot_reload=True)
    await cascade.start_watching()
    await cascade.stop_watching()
    # Second stop is a no-op, never raises.
    await cascade.stop_watching()


def test_watcher_without_dirs_is_safe(tmp_path: Path) -> None:
    """HotReloadWatcher gracefully ignores loaders with no root."""
    cascade = VariableCascade(None, None, None, None, hot_reload=True)
    # Never start; just exercise the resolve path to confirm a
    # cascade with no dirs still answers fixed keys.
    assert cascade.resolve("Date", model_name="x") is not None
    # And that an explicit watcher build over empty loaders is inert.
    w = HotReloadWatcher([])
    assert w is not None
