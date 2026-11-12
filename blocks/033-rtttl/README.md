# 033 — RTTTL Melodies & Tone Synthesis

**Status:** ❌ not possible yet
**ESPHome lego:** `rtttl:` + `output: platform: ledc`

## ESPHome

```yaml
output:
  - platform: ledc
    pin: GPIO7
    id: buzzer_pwm

rtttl:
  output: buzzer_pwm

button:
  - platform: template
    name: "Play Alert Tune"
    on_press:
      - rtttl.play: "beep:d=4,o=5,b=200:8c6,8g,8c6"
```

RTTTL is a small text notation: default duration, octave and tempo, then a list
of notes with optional duration, pitch and dotted flags. Playing it means
retuning a PWM output for each note and holding it for the note's length.

## espforge

`rtttl.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `output: platform: ledc, pin` | proposed `<pwm id="buzzer" pin="7" />`; block 008 has no PWM either |
| `rtttl: output` | proposed `<rtttl id="tunes" output="$buzzer" />` |
| `rtttl.play: "..."` | proposed `service!(ctx, tunes).play("beep:d=4,o=5,b=200:8c6,8g,8c6")` |
| note → frequency | none: no timer retune, no pitch table |
| duration → time | none |
| `button: platform: template` | none: a `<button>` is a physical pin, not an entity action |
| `name:` | none |

A buzzer is the one output where software toggling is least viable: even a
single 440 Hz tone needs a 1.1 ms half-period, and a melody needs that period to
change on exact note boundaries while the rest of the app keeps running.

### What is missing

A PWM output with a runtime-settable frequency (not just duty), an RTTTL parser,
and a non-blocking note scheduler. Template buttons are a separate gap shared
with blocks 009 and 036.
