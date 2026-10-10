"""Print the cast timeline the game uses for a skill (our own tool).

The skill table file gsdata_en.dat holds, besides the 144-byte skill records,
an "action table" at byte 0x1B00C: 574 rows of 57 i16 (0x72 bytes). A skill's
s12 picks its row; s13 picks the row used for a mid-air cast (see s40).
Column meanings were read from the game program (docs/PROGRAM-RE.md).

Usage:  python engine/re/action_table.py <gsdata_en.dat> <skills.json> <skill id> [...]
"""
import json
import struct
import sys

ACTION_TABLE = 0x1B00C
ROW = 0x72
ANIM = [  # the program's motion-name table at 0x1402db400 (136 entries; index = animation id)
    "WAIT0", "WAIT1", "WAIT1IN", "WAIT1OUT", "WAIT_ACT0", "WAIT_CON", "WAIT_DAM", "WALK", "WALK_CON", "WALK_DAM",
    "WALK_FAST", "WALK_SLOW", "RUN", "RUN_CON", "RUN_DAM", "RUN_FAST", "RUN_SLOW", "TURN_WALK_R", "TURN_WALK_L",
    "TURN_WALK_BR", "TURN_WALK_BL", "TURN_RUN_R", "TURN_RUN_L", "TURN_RUN_BR", "TURN_RUN_BL", "TURN_WDAM_R",
    "TURN_WDAM_L", "TURN_WDAM_BR", "TURN_WDAM_BL", "TURN_RDAM_R", "TURN_RDAM_L", "TURN_RDAM_BR", "TURN_RDAM_BL",
    "DASH_A", "DASHEND_A", "DASH_B", "DASHEND_B", "JUMP_START", "JUMP_UP", "JUMP_DOWN", "JUMP_LAND0", "JUMP_LAND1",
    "FLOAT_IN", "FLOAT_WAIT", "FLOAT_MOVE", "BLOW0_FRONT", "BLOW1_FRONT", "BLOW2_FRONT", "BLOW0_BACK", "BLOW1_BACK",
    "BLOW_LAND_LF", "BLOW_LAND_MF", "BLOW_LAND_SF", "BLOW_LAND_LB", "BLOW_LAND_MB", "BLOW_LAND_SB", "WALLHIT_FRONT",
    "WALLHIT_FRONT_LOOP", "WALLHIT_BACK", "WALLHIT_BACK_LOOP", "DOWN_FRONT", "DOWN_FRONT_STAND", "DOWN_BACK",
    "DOWN_BACK_STAND", "ATK_SORD", "ATK_SHOT", "DEF_SHIELD", "ATK_WHAND_LASER", "ATK_SKYSHOT_LOW",
    "ATK_SKYSHOT_HIGH", "ATK_KI_SHOT", "ATK_WHAND_SORD", "DEF_BRUSH", "DEF_ABSORB", "ATK_STAMP", "DEF_SHELTER",
    "ATK_LASER", "SPE_DASH", "SPE_WARP", "ATK_SPHERE", "ATK_THROW", "ATK_SPIKE", "ATK_WHAND_SPIKE", "ATK_GROUND",
    "DEF_WHAND_SHIELD", "ATK_DRAIN", "SPE_POWER_UP", "DEF_COURSE", "ATK_SET_BOMB", "SPE_RESET", "ATK_TUKI",
    "ATK_PUNCH", "ATK_SUPER_SHOT", "SPE_HEAL", "ATK_KICK", "ATK_BODY", "ENV_SET", "ATK_FAITH_SHOT", "ATK_PUNCH_2",
    "ATK_PUNCH_3", "ATK_LASER_2", "ATK_LASER_3", "DMG_RAISE_HIGH", "DMG_DRAIN_HIGH", "DMG_SPIKE_HIGH",
    "DMG_BIND_AIR", "DMG_CONFUSION", "DMG_BIND_CROSS", "DMG_BIND_CHAIN", "DMG_BIND_PRESS", "DMG_AURA",
    "DMG_STAMP_FACEUP", "DMG_ABILITY", "DMG_STATUS_UP", "DMG_STATUS_DOWN", "DMG_STATUS_RESET", "DMG_PARARISE",
    "DMG_SHOT_FRONT", "DMG_SHOT_BACK", "DMG_SHOT_LEFT", "DMG_SHOT_RIGHT", "DMG_SPHERE_FRONT", "DMG_SPHERE_BACK",
    "DMG_SORD_FRONT", "DMG_SORD_BACK", "DMG_DRAIN_LOW", "DMG_SPIKE_LOW", "DMG_RAISE_LOW", "DMG_STAMP_FACEDOWN",
    "DMG_SHORT", "EXMOT0", "EXMOT1", "EXMOT2", "EXMOT3", "EXMOT4", "EXMOT5",
]


def row(data, i):
    return struct.unpack_from("<57h", data, ACTION_TABLE + i * ROW)


def describe(w):
    anim = ANIM[w[38]] if 0 <= w[38] < len(ANIM) else f"#{w[38]}"
    out = [
        f"valid={w[0]} effect=eft{w[2]:04d} cast_sound=common/{w[3]} voice_group={w[4]}",
        f"anim={w[38]} {anim} start_frame={w[40]} fire_tick={w[43]} (30 Hz)",
    ]
    if w[41] or w[42] or w[44]:
        out.append(f"dash ticks {w[41]}..{w[42]}  hit window ticks {w[43]}..{w[44]}")
    # Controller rumble (pdvkFeedBack): cast block w6..w25, fire block w26..w37.
    out.append(f"rumble at cast {w[6:26]}  rumble at fire {w[26:38]}")
    return out


def hidden(data, record):
    """s70..s73: the last 8 bytes of the program's 0x90-byte record = the first 8 bytes of the
    next record in our framing (file offset 160 + 144 * (record + 1)). Not in skills.json raw."""
    return struct.unpack_from("<4h", data, 160 + 144 * (record + 1))


def main():
    data = open(sys.argv[1], "rb").read()
    skills = json.load(open(sys.argv[2]))["skills"]
    for sid in map(int, sys.argv[3:]):
        s = next(x for x in skills if x["id"] == sid)
        r = s["raw"]
        print(f"{sid} {s['name']}: motion s63={r[63]} s64..s69={r[64:70]} s70..s73={hidden(data, s['record'])} "
              f"s40={r[40]} s44={r[44]} s57..s62={r[57:63]}")
        for label, k in (("ground", r[12]), ("mid-air", r[13])):
            if k > 0:
                print(f"  {label} row {k}:")
                for line in describe(row(data, k)):
                    print("    " + line)


if __name__ == "__main__":
    main()
