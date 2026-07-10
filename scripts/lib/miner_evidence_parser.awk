# Parse PulseDAG miner terminal evidence from log files.
# Submission counters are derived exclusively from terminal `submit_result:` lines.
function shell_quote(s) { gsub(/'/, "'\"'\"'", s); return "'" s "'" }
function json_escape(s) { gsub(/\\/, "\\\\", s); gsub(/"/, "\\\"", s); return s }
function field_value(line, key,    re, m) {
  re = "(^|[[:space:]])" key "=([^[:space:]]+)"
  if (match(line, re, m)) return m[2]
  return ""
}
function add_failure(code, detail) { failures[++failure_count] = code ": " detail }
/submit_result:/ {
  total++
  accepted = field_value($0, "accepted")
  rejected = field_value($0, "rejected")
  reason = field_value($0, "reason_code")
  block_hash = field_value($0, "block_hash")
  height = field_value($0, "height")
  if (accepted == "true") accepted_total++
  if (rejected == "true") {
    rejected_total++
    if (reason == "" || reason == "none" || reason == "null") reason = "unknown"
    rejected_by_reason[reason]++
  }
  if (block_hash != "" && block_hash != "none" && block_hash != "null") unique_hashes[block_hash] = 1
  next
}
/template_received|received_template|new_template|template:/ { templates++ }
END {
  unique_count = 0
  for (h in unique_hashes) unique_count++
  reason_sum = 0
  for (r in rejected_by_reason) reason_sum += rejected_by_reason[r]
  if (total != accepted_total + rejected_total) add_failure("MINER_SUBMIT_TOTAL_MISMATCH", "total != accepted + rejected")
  if (accepted_total > total) add_failure("MINER_SUBMIT_ACCEPTED_EXCEEDS_TOTAL", "accepted exceeds total")
  if (rejected_total > total) add_failure("MINER_SUBMIT_REJECTED_EXCEEDS_TOTAL", "rejected exceeds total")
  if (reason_sum != rejected_total) add_failure("MINER_SUBMIT_REASON_COUNT_MISMATCH", "reason counts do not sum to rejected")
  if (unique_count > total) add_failure("MINER_SUBMIT_UNIQUE_HASHES_EXCEED_TOTAL", "unique submitted hashes exceed total")

  reasons = "["; sep = ""
  for (r in rejected_by_reason) { reasons = reasons sep "{\"reason\":\"" json_escape(r) "\",\"count\":" rejected_by_reason[r] "}"; sep = "," }
  reasons = reasons "]"
  failure_json = "["; sep = ""
  for (i = 1; i <= failure_count; i++) { failure_json = failure_json sep "\"" json_escape(failures[i]) "\""; sep = "," }
  failure_json = failure_json "]"

  print "local_miner_templates_received=" templates + 0
  print "local_miner_submits_total=" total + 0
  print "local_miner_submits_accepted=" accepted_total + 0
  print "local_miner_submits_rejected=" rejected_total + 0
  print "local_miner_submits_rejected_by_reason=" shell_quote(reasons)
  print "unique_submitted_block_hashes=" unique_count + 0
  print "miner_evidence_consistency_failures=" shell_quote(failure_json)
}
