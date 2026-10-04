#!/bin/sh
set -eu

usage() {
    cat >&2 <<'EOF'
usage: $0 <version|test|coverage> [options]

Render one deterministic SVG badge. Defaults:
  version   --text version  --value unknown  --color #007ec6
  test      --text tests    --value passing  --color #4c1
  coverage  --text coverage --value -        --color #9f9f9f --suffix %

Options:
  --text TEXT          left-hand label
  --value VALUE        right-hand value
  --color COLOR        value background (#RGB or #RRGGBB)
  --size PX            font size, from 6 through 24 (default: 11)
  --height PX          badge height, from 14 through 64 (default: 20)
  --label-width PX     fixed label width instead of automatic sizing
  --value-width PX     fixed value width instead of automatic sizing
  --suffix TEXT        text appended to the displayed value
  --output PATH        output file (default: coverage/<kind>.svg)
  -h, --help           show this help

Examples:
  ./ci-coverage-badges.sh version --value 1.2.3
  ./ci-coverage-badges.sh test --value failing
  ./ci-coverage-badges.sh coverage --value 87.4
  ./ci-coverage-badges.sh version --text release --value rc.12 --size 12 --color '#8a2be2'
EOF
    exit "${1:-2}"
}

[ "$#" -ge 1 ] || usage
kind=$1
shift

case "$kind" in
    version)
        text=version
        value=unknown
        color='#007ec6'
        suffix=
        output=coverage/version.svg
        ;;
    test)
        text=tests
        value=passing
        color=
        suffix=
        output=coverage/test.svg
        ;;
    coverage)
        text=coverage
        value=-
        color=
        suffix=%
        output=coverage/coverage.svg
        ;;
    -h|--help) usage 0 ;;
    *)
        echo "unsupported badge kind: $kind" >&2
        usage
        ;;
esac

font_size=11
height=20
label_width=
value_width=
color_set=false

while [ "$#" -gt 0 ]; do
    option=$1
    case "$option" in
        --text|--value|--color|--size|--height|--label-width|--value-width|--suffix|--output)
            [ "$#" -ge 2 ] || {
                echo "missing value for $option" >&2
                exit 2
            }
            option_value=$2
            shift 2
            case "$option" in
                --text) text=$option_value ;;
                --value) value=$option_value ;;
                --color) color=$option_value; color_set=true ;;
                --size) font_size=$option_value ;;
                --height) height=$option_value ;;
                --label-width) label_width=$option_value ;;
                --value-width) value_width=$option_value ;;
                --suffix) suffix=$option_value ;;
                --output) output=$option_value ;;
            esac
            ;;
        -h|--help) usage 0 ;;
        *)
            echo "unknown option: $option" >&2
            usage
            ;;
    esac
done

validate_text() {
    field_name=$1
    field_value=$2
    if ! printf '%sX\n' "$field_value" | LC_ALL=C awk '
        BEGIN { valid = 1 }
        NR != 1 || $0 !~ /^[ -~]+X$/ || length($0) <= 1 { valid = 0 }
        END { exit(valid == 0 ? 1 : 0) }
    '; then
        echo "$field_name must contain printable ASCII characters: $field_value" >&2
        exit 2
    fi
}

validate_integer() {
    field_name=$1
    field_value=$2
    minimum=$3
    maximum=$4
    case "$field_value" in
        ''|*[!0-9]*)
            echo "$field_name must be an integer from $minimum through $maximum: $field_value" >&2
            exit 2
            ;;
    esac
    if ! awk -v value="$field_value" -v low="$minimum" -v high="$maximum" 'BEGIN { exit (value >= low && value <= high ? 0 : 1) }'; then
        echo "$field_name must be an integer from $minimum through $maximum: $field_value" >&2
        exit 2
    fi
}

validate_text text "$text"
validate_text value "$value"
if [ -n "$suffix" ]; then
    validate_text suffix "$suffix"
fi
validate_integer size "$font_size" 6 24
validate_integer height "$height" 14 64
if [ -n "$label_width" ]; then
    validate_integer label-width "$label_width" 20 1000
fi
if [ -n "$value_width" ]; then
    validate_integer value-width "$value_width" 20 1000
fi
case "$output" in
    ''|*/) echo "output must name an SVG file: $output" >&2; exit 2 ;;
esac

case "$kind" in
    test)
        if [ "$color_set" = false ]; then
            case "$value" in
                passing) color='#4c1' ;;
                failing) color='#e05d44' ;;
                *) color='#9f9f9f' ;;
            esac
        fi
        ;;
    coverage)
        if [ "$value" = '-' ]; then
            [ "$color_set" = true ] || color='#9f9f9f'
        else
            if ! awk -v candidate="$value" 'BEGIN {
                valid = candidate ~ /^[0-9]+([.][0-9]+)?$/ && candidate >= 0 && candidate <= 100
                exit(valid ? 0 : 1)
            }'; then
                echo "coverage value must be - or a number from 0 through 100: $value" >&2
                exit 2
            fi
            value=$(awk -v candidate="$value" 'BEGIN { printf "%.0f", candidate }')
            if [ "$color_set" = false ]; then
                if [ "$value" -ge 90 ]; then
                    color='#4c1'
                elif [ "$value" -ge 70 ]; then
                    color='#dfb317'
                else
                    color='#e05d44'
                fi
            fi
        fi
        ;;
esac

case "$color" in
    \#[0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f]|\#[0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f]) ;;
    *) echo "color must be #RGB or #RRGGBB: $color" >&2; exit 2 ;;
esac

display_value=$value$suffix
if [ -z "$label_width" ]; then
    label_width=$(printf '%s\n' "$text" | awk -v size="$font_size" '{
        width = int(length($0) * size * 0.64 + 14)
        print (width < 35 ? 35 : width)
    }')
fi
if [ -z "$value_width" ]; then
    value_width=$(printf '%s\n' "$display_value" | awk -v size="$font_size" '{
        width = int(length($0) * size * 0.64 + 14)
        print (width < 35 ? 35 : width)
    }')
fi

badge_width=$((label_width + value_width))
label_center=$(awk -v width="$label_width" 'BEGIN { printf "%.1f", width / 2 }')
value_center=$(awk -v left="$label_width" -v width="$value_width" 'BEGIN {
    printf "%.1f", left + width / 2
}')
vertical_center=$(awk -v badge_height="$height" 'BEGIN { printf "%.1f", badge_height / 2 }')

xml_escape() {
    sed \
        -e 's/&/\&amp;/g' \
        -e 's/</\&lt;/g' \
        -e 's/>/\&gt;/g' \
        -e 's/"/\&quot;/g'
}

sed_replacement_escape() {
    sed -e 's/[\\&|]/\\&/g'
}

escaped_text=$(printf '%s' "$text" | xml_escape | sed_replacement_escape)
escaped_value=$(printf '%s' "$display_value" | xml_escape | sed_replacement_escape)
escaped_title=$(printf '%s: %s' "$text" "$display_value" | xml_escape | sed_replacement_escape)

[ ! -d "$output" ] || { echo "output must not be a directory: $output" >&2; exit 2; }
output_dir=${output%/*}
if [ "$output_dir" != "$output" ]; then
    mkdir -p "$output_dir"
fi
temporary=$(mktemp "${output}.tmp.XXXXXX")
trap 'rm -f "$temporary"' 0
trap 'exit 1' HUP INT TERM

sed \
    -e "s/@BADGE_WIDTH@/$badge_width/g" \
    -e "s/@HEIGHT@/$height/g" \
    -e "s/@LABEL_WIDTH@/$label_width/g" \
    -e "s/@VALUE_WIDTH@/$value_width/g" \
    -e "s/@LABEL_CENTER@/$label_center/g" \
    -e "s/@VALUE_CENTER@/$value_center/g" \
    -e "s/@VERTICAL_CENTER@/$vertical_center/g" \
    -e "s/@FONT_SIZE@/$font_size/g" \
    -e "s/@COLOR@/$color/g" \
    -e "s|@TITLE@|$escaped_title|g" \
    -e "s|@TEXT@|$escaped_text|g" \
    -e "s|@VALUE@|$escaped_value|g" \
    >"$temporary" <<'EOF'
<svg xmlns="http://www.w3.org/2000/svg" width="@BADGE_WIDTH@" height="@HEIGHT@" role="img" aria-label="@TITLE@">
  <title>@TITLE@</title>
  <linearGradient id="s" x2="0" y2="100%">
    <stop offset="0" stop-color="#fff" stop-opacity=".7"/>
    <stop offset=".1" stop-color="#aaa" stop-opacity=".1"/>
    <stop offset=".9" stop-opacity=".3"/>
    <stop offset="1" stop-opacity=".5"/>
  </linearGradient>
  <clipPath id="r"><rect width="@BADGE_WIDTH@" height="@HEIGHT@" rx="3"/></clipPath>
  <g clip-path="url(#r)">
    <rect width="@LABEL_WIDTH@" height="@HEIGHT@" fill="#555"/>
    <rect x="@LABEL_WIDTH@" width="@VALUE_WIDTH@" height="@HEIGHT@" fill="@COLOR@"/>
    <rect width="@BADGE_WIDTH@" height="@HEIGHT@" fill="url(#s)"/>
  </g>
  <g fill="#fff" text-anchor="middle" dominant-baseline="middle" font-family="Verdana,Geneva,DejaVu Sans,sans-serif" font-size="@FONT_SIZE@">
    <text x="@LABEL_CENTER@" y="@VERTICAL_CENTER@">@TEXT@</text>
    <text x="@VALUE_CENTER@" y="@VERTICAL_CENTER@">@VALUE@</text>
  </g>
</svg>
EOF

mv "$temporary" "$output"
trap - 0 HUP INT TERM
