"""Bundled starter skills shipped with corlinman-server.

Each ``*.md`` next to this file is a SKILL card in the corlinman format
(YAML frontmatter + Markdown body, parsed by ``corlinman_skills_registry``).
They are copied into ``<data_dir>/profiles/default/skills/`` on first
boot by :mod:`corlinman_server.gateway.lifecycle.starter_skills` so a
freshly-installed gateway has a working starter library out of the box.

The directory ships as package data; Hatch's default wheel target
(``packages = ["src/corlinman_server"]``) picks the ``.md`` files up
automatically. Override the location at runtime with the
``CORLINMAN_BUNDLED_SKILLS_DIR`` environment variable.
"""
