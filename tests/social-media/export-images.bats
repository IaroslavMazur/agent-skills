#!/usr/bin/env bats
# Bats supplies its test-directory variables at runtime.
# shellcheck disable=SC2154

setup() {
  exporter="$BATS_TEST_DIRNAME/../../skills/social-media/scripts/export-images.sh"
  master="$BATS_TEST_TMPDIR/master image.png"
  magick -size 40x20 xc:red -fill blue -draw 'rectangle 20,0 39,19' "$master"
}

@test "all presets export exact dimensions and leave the master unchanged" {
  before=$(shasum "$master")
  run /bin/bash "$exporter" "$master" "$BATS_TEST_TMPDIR/all"
  [ "$status" -eq 0 ]
  [ "${#lines[@]}" -eq 9 ]
  [[ "${lines[0]}" = 'og.jpg 1200x630 '* ]]
  while read -r preset dimensions; do
    [ "$(magick identify -format '%wx%h' "$BATS_TEST_TMPDIR/all/$preset.jpg")" = "$dimensions" ]
  done < <(/bin/bash "$exporter" --list)
  [ "$(shasum "$master")" = "$before" ]
}

@test "pad preserves both edges and crop gravity selects the requested edge" {
  /bin/bash "$exporter" "$master" "$BATS_TEST_TMPDIR/pad" 20x20 --format png --background white
  [ "$(magick "$BATS_TEST_TMPDIR/pad/20x20.png" -format '%[pixel:p{0,0}]' info:)" = 'srgb(255,255,255)' ]
  [ "$(magick "$BATS_TEST_TMPDIR/pad/20x20.png" -format '%[pixel:p{2,10}]' info:)" = 'srgb(255,0,0)' ]
  [ "$(magick "$BATS_TEST_TMPDIR/pad/20x20.png" -format '%[pixel:p{17,10}]' info:)" = 'srgb(0,0,255)' ]
  /bin/bash "$exporter" "$master" "$BATS_TEST_TMPDIR/crop" 20x20 --format png --fit crop --gravity East
  [ "$(magick "$BATS_TEST_TMPDIR/crop/20x20.png" -format '%[pixel:p{0,10}]' info:)" = 'srgb(0,0,255)' ]
}

@test "PNG preserves transparency and JPEG uses the requested matte" {
  magick -size 20x10 xc:none -fill red -draw 'rectangle 0,0 9,9' "$master"
  /bin/bash "$exporter" "$master" "$BATS_TEST_TMPDIR/alpha" 20x20 --format png --background none
  [ "$(magick "$BATS_TEST_TMPDIR/alpha/20x20.png" -format '%[fx:p{19,10}.a]' info:)" = 0 ]
  /bin/bash "$exporter" "$master" "$BATS_TEST_TMPDIR/matte" 20x20 --background white --quality 100
  [ "$(magick "$BATS_TEST_TMPDIR/matte/20x20.jpg" -format '%[fx:p{19,10}.r>0.99 && p{19,10}.g>0.99 && p{19,10}.b>0.99]' info:)" = 1 ]
}

@test "existing directories and invalid arguments fail before exporting" {
  mkdir "$BATS_TEST_TMPDIR/existing"
  printf 'keep me' > "$BATS_TEST_TMPDIR/existing/og.jpg"
  run /bin/bash "$exporter" "$master" "$BATS_TEST_TMPDIR/existing" og
  [ "$status" -ne 0 ]
  [ "$(cat "$BATS_TEST_TMPDIR/existing/og.jpg")" = 'keep me' ]
  run /bin/bash "$exporter" "$master" "$BATS_TEST_TMPDIR/invalid" og nonexistent
  [ "$status" -ne 0 ]
  [ ! -e "$BATS_TEST_TMPDIR/invalid" ]
  run /bin/bash "$exporter" "$master" "$BATS_TEST_TMPDIR/invalid" 9999x2
  [ "$status" -ne 0 ]
  run /bin/bash "$exporter" "$master" "$BATS_TEST_TMPDIR/invalid" --quality
  [ "$status" -ne 0 ]
}

@test "animated sources produce exactly one still image" {
  magick -size 10x10 xc:red -size 10x10 xc:blue "$BATS_TEST_TMPDIR/animated.gif"
  run /bin/bash "$exporter" "$BATS_TEST_TMPDIR/animated.gif" "$BATS_TEST_TMPDIR/still" 10x10
  [ "$status" -eq 0 ]
  [ "${#lines[@]}" -eq 1 ]
  [ -f "$BATS_TEST_TMPDIR/still/10x10.jpg" ]
}
