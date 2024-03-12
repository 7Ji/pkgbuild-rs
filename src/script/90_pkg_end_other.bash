        echo END
      done
    fi
    if [[ "${arch[*]}" != "${_arch_collapsed}" ]]; then
      arch=("${_arch_backup[@]}")
    fi
    echo END
  ) || exit $?
  done
