#!/usr/bin/env bash
# Writes a JUnit step report that pr-results reads, for one CI step.
#
# (c) Copyright 2026 Liminal HQ, Scott Morris
# SPDX-License-Identifier: Apache-2.0 OR MIT

set -euo pipefail

# Usage:
#   step-report.sh run OUT_DIR JOB TITLE -- COMMAND [ARG...]
#   step-report.sh outcome OUT_DIR JOB TITLE STATUS
#
# `run` runs the command, shows its output as usual, writes OUT_DIR/steps.xml and exits with the
# command's status. A failing step carries the last lines of its output. `outcome` writes the
# report for a step that ran elsewhere (an action), from its `success`, `failure`, `cancelled` or
# `skipped` outcome. The format is the step-results contract in pr-results' SPEC.md: one test suite
# per job, one test case per step.

tail_lines=80

usage() {
	echo "usage: step-report.sh run OUT_DIR JOB TITLE -- COMMAND [ARG...]" >&2
	echo "       step-report.sh outcome OUT_DIR JOB TITLE STATUS" >&2
	exit 2
}

# Escapes text for an XML attribute or body and drops characters XML 1.0 cannot hold.
xml_escape() {
	LC_ALL=C tr -d '\000-\010\013\014\016-\037' |
		sed -e 's/&/\&amp;/g' -e 's/</\&lt;/g' -e 's/>/\&gt;/g' -e 's/"/\&quot;/g'
}

esc() {
	printf '%s' "$1" | xml_escape
}

write_report() {
	local out="$1" job="$2" title="$3" status="$4" seconds="$5" log="$6" message="$7"
	local failures=0 skipped=0
	case "${status}" in
	failure) failures=1 ;;
	cancelled | skipped) skipped=1 ;;
	esac
	mkdir -p "${out}"
	{
		echo '<?xml version="1.0" encoding="UTF-8"?>'
		echo "<testsuites name=\"$(esc "${job}")\" tests=\"1\" failures=\"${failures}\" time=\"${seconds}\">"
		echo "	<testsuite name=\"$(esc "${job}")\" tests=\"1\" failures=\"${failures}\" skipped=\"${skipped}\" time=\"${seconds}\">"
		printf '		<testcase classname="ci.%s" name="%s" time="%s"' "$(esc "${job}")" "$(esc "${title}")" "${seconds}"
		if [ "${status}" = "failure" ]; then
			printf '>\n			<failure message="%s" type="step">' "$(esc "${message}")"
			if [ -n "${log}" ] && [ -f "${log}" ]; then
				tail -n "${tail_lines}" "${log}" | xml_escape
			fi
			printf '</failure>\n		</testcase>\n'
		elif [ "${skipped}" = 1 ]; then
			printf '>\n			<skipped message="%s"/>\n		</testcase>\n' "$(esc "${message}")"
		else
			printf '/>\n'
		fi
		echo '	</testsuite>'
		echo '</testsuites>'
	} >"${out}/steps.xml"
}

[ "$#" -ge 5 ] || usage
mode="$1"
out="$2"
job="$3"
title="$4"
shift 4

case "${mode}" in
run)
	[ "$1" = "--" ] || usage
	shift
	[ "$#" -ge 1 ] || usage
	log="$(mktemp)"
	trap 'rm -f "${log}"' EXIT
	start="$(date +%s.%N)"
	set +e
	"$@" 2>&1 | tee "${log}"
	code="${PIPESTATUS[0]}"
	set -e
	seconds="$(awk -v s="${start}" -v e="$(date +%s.%N)" 'BEGIN { printf "%.3f", e - s }')"
	if [ "${code}" -eq 0 ]; then
		write_report "${out}" "${job}" "${title}" success "${seconds}" "" ""
	else
		write_report "${out}" "${job}" "${title}" failure "${seconds}" "${log}" "exit status ${code}"
	fi
	exit "${code}"
	;;
outcome)
	status="$1"
	case "${status}" in
	success | failure | cancelled | skipped) ;;
	*) usage ;;
	esac
	case "${status}" in
	failure) message="step failed" ;;
	cancelled) message="step cancelled" ;;
	skipped) message="not run: an earlier step failed" ;;
	*) message="" ;;
	esac
	write_report "${out}" "${job}" "${title}" "${status}" 0 "" "${message}"
	;;
*)
	usage
	;;
esac
