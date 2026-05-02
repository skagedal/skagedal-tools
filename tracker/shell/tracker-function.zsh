function work() {
  tracker start
}

function wedit() {
  date +%H:%M | tr -d '\n' | pbcopy
  tracker edit
}

function wreport() {
  tracker report
}

function wstop() {
  tracker stop
}
