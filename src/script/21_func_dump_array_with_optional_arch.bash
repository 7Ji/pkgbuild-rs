dump_array_with_optional_arch() { #1: var name, 2: report name
  declare -n array="$1"
  declare -n array_arch="$1_${CARCH}"
  local item
  for item in "${array[@]}" "${array_arch[@]}"; do
    echo "$2:${item}"
  done
}
