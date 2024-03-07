        echo END
      done
    fi
    if [[ "${arch[*]}" != "${_arch_collapsed}" ]]; then
      arch=("${_arch_backup[@]}")
    fi
    echo END
  done
  unset -f pkgver package{,_"${pkgbase}"} "${pkgname[@]/#/package_}"
  unset -v pkgname arch