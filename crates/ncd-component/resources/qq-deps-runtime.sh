# 在一个只读会话中查询整份目录，避免逐个包承担 SSH 往返。
LC_ALL=C
export LC_ALL
family=$1
shift
case "$family" in
    debian) command -v dpkg-query >/dev/null 2>&1 || exit 2 ;;
    rhel) command -v rpm >/dev/null 2>&1 || exit 2 ;;
    *) exit 2 ;;
esac
printf '{"v":1,"packages":['
separator=
while [ "$#" -gt 0 ]; do
    [ "$#" -ge 2 ] || exit 2
    base=$1
    alias=$2
    shift 2
    name=$base
    if [ "$family" = debian ] && [ "$alias" = 1 ]; then
        command -v apt-cache >/dev/null 2>&1 || exit 2
        apt-cache show "${base}t64" >/dev/null 2>&1
        result=$?
        case "$result" in
            0) name=${base}t64 ;;
            100) : ;;
            *) exit 2 ;;
        esac
    fi
    installed=false
    if [ "$family" = debian ]; then
        status=$(dpkg-query -W '-f=${Status}' "$name" 2>/dev/null)
        result=$?
        case "$result" in
            0) [ "$status" != 'install ok installed' ] || installed=true ;;
            1) : ;;
            *) exit 2 ;;
        esac
    else
        status=$(rpm -q "$name" 2>/dev/null)
        result=$?
        case "$result" in
            0) installed=true ;;
            1) [ "$status" = "package $name is not installed" ] || exit 2 ;;
            *) exit 2 ;;
        esac
    fi
    printf '%s{"base":"%s","name":"%s","installed":%s}' "$separator" "$base" "$name" "$installed"
    separator=,
done
printf ']}\n'
