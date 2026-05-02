export SUGGESTED_CD_FILE=~/.assistant/data/requested-directory

function n() {
  rm $SUGGESTED_CD_FILE &> /dev/null
  if [ $# -gt 0 ]; then
    assistant run "$1"
  else
    assistant next
  fi
  if [ -e $SUGGESTED_CD_FILE ]; then
    cd "$(<$SUGGESTED_CD_FILE)"
  fi
}
