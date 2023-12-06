dump_array() { #1: var name, 2: report name
  declare -n array="$1"
  local item
  for item in "${array[@]}"; do
    echo "$2:${item}"
  done
}
