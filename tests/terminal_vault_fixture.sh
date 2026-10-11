#!/bin/sh
case "$1 $2" in
  'read -format=json')
    printf '%s' '{"data":{"secret":{"secret/":{"type":"kv"},"pki/":{"type":"pki"}}}}'
    ;;
  'kv list')
    [ "$3 $4" = '-format=json secret' ] || exit 3
    printf '%s' '["folder/"]'
    ;;
  'kv get')
    [ "$3 $4" = '-format=json secret' ] || exit 4
    printf '%s' '{"data":{"data":{"password":"NEVER_RENDER_VAULT_VALUE"}}}'
    ;;
  *) exit 9 ;;
esac
