#!/usr/bin/env python3
"""Emit owned read-only command-identity probes; never launch a game or infer encoding."""
import json

NAMES = ("SetCritDam", "SetWeaponCritDamage", "IsPlayerSwimming",
         "DisableHardcoreTracking", "babby")


def identity_steps():
    return {
        "format": "fnv.capture.steps/1",
        "name": "critical_player_command_identity",
        "title": "Critical damage and player-state command identity",
        "start": "any save; queries do not change state",
        "steps": [
            {"query": f'Begin Function {{}}\nSetFunctionValue (GetCommandOpcode "{name}")\nEnd\n',
             "label": f"identity.{name.lower()}", "field": "value"}
            for name in NAMES
        ],
    }


if __name__ == "__main__":
    print(json.dumps(identity_steps(), indent=1))
