# Milestone 20: windows and screens

Implemented on 2026-09-22.

| Piece | Contract |
| --- | --- |
| `AppOptions::placement` | Which screen a window opens on, and where. `System` leaves it to the platform, `Centered` puts it in the middle of the primary screen, `WithOwner` puts it in the middle of the screen its owner is on, so a dialog stays with the window it belongs to, and `Named("DELL")` matches a screen name, ignoring case, falling back to the primary one when nothing matches, so unplugging a monitor cannot hide a window. |
| Centring | By the frame the user sees, which is only known once the window exists, and in the screen's own pixels. Everything else about several monitors was already handled: each window follows the scale factor of the screen it is on. |

## Validation

Measured on this machine, which has one screen: a window opened with `Placement::System` landed at 662, 549 while the screen's centre is 1024, 576; the same window with `Placement::Centered` landed at 1023, 576, and with `Placement::Named("NVIDIA")` at 1023, 576 as well. The counter example takes `--centered` and `--screen <name>` so the check can be repeated.

**Not verified:** a second monitor. The code that picks a screen is the same either way, but choosing between screens, and a dialog following its owner to another one, were not run here.
