#!/bin/sh
# Dev helper: horizon-ctl of the test build, on the game's default control socket.
exec "$HOME/AppData/Roaming/horizon-launcher/versions/${VH_VERSION:-native-c9d474d-b722baeeea54}/horizon-ctl.exe" "$@"
