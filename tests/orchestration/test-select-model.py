from __future__ import annotations

import copy
import http.client
import importlib.util
import io
import json
import os
import signal
import subprocess
import sys
import unittest
import urllib.error
from pathlib import Path
from unittest.mock import Mock, patch

SCRIPT = Path(__file__).resolve().parents[2] / "skills/orchestration/scripts/select-model.py"
SPEC = importlib.util.spec_from_file_location("select_model", SCRIPT)
ROUTER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(ROUTER)
PAYLOAD = {
    "task": "Implement a bounded parser correction and its regression test.",
    "context": "Implementation. Native Codex. No external writes.",
    "candidates": [
        {"id": "routine", "model": "gpt-6-luna", "effort": "high", "description": "Routine work."},
        {"id": "involved", "model": "gpt-6.1-sol", "effort": "medium", "description": "Involved work."},
    ],
}
ANSWER = {
    "answers": {
        "configuration": {
            "type": "score",
            "score": 0.9,
            "confidence": 0.8,
            "probabilities": {"0": 0.1, "1": 0.9},
        }
    }
}


def ladder_payload(count):
    payload = copy.deepcopy(PAYLOAD)
    payload["candidates"] = [{**PAYLOAD["candidates"][0], "id": f"tier_{index}"} for index in range(count)]
    return payload


def ladder_answer(levels, confidence=None):
    """Build a Score answer whose confidence follows Jev's formula unless overridden."""
    if confidence is None:
        confidence = ROUTER.score_confidence(levels, levels.index(max(levels)))[0]
    return {
        "answers": {
            "configuration": {
                "type": "score",
                "confidence": confidence,
                "probabilities": {str(index): value for index, value in enumerate(levels)},
            }
        }
    }


class SelectModelTests(unittest.TestCase):
    def call(self, answer=ANSWER, status=200, error=None, payload=PAYLOAD):
        response = Mock()
        response.status = status
        response.read.return_value = answer if isinstance(answer, bytes) else json.dumps(answer).encode()
        response.__enter__ = Mock(return_value=response)
        response.__exit__ = Mock(return_value=False)
        opener = Mock()
        opener.open.side_effect = error
        opener.open.return_value = response
        with patch.object(ROUTER.urllib.request, "build_opener", return_value=opener):
            result = ROUTER.select_configuration(payload, "test-placeholder")
        return result, opener, response

    def test_success_uses_ordered_ladder_and_evaluate_wire(self):
        result, opener, response = self.call()
        self.assertEqual(
            result,
            {
                "status": "selected",
                "id": "involved",
                "model": "gpt-6.1-sol",
                "effort": "medium",
                "confidence": 0.8,
                "rule": "confidence",
            },
        )
        opener.open.assert_called_once()
        request = opener.open.call_args.args[0]
        self.assertEqual(request.full_url, "https://ai-gateway.vercel.sh/v1/evaluate")
        self.assertEqual(request.method, "POST")
        self.assertEqual(request.get_header("Authorization"), "Bearer test-placeholder")
        body = json.loads(request.data)
        self.assertEqual(body["model"], "typesafe-ai/jev")
        question = body["questions"]["configuration"]
        self.assertEqual(question["type"], "score")
        self.assertEqual(len(question["criteria"]), 2)
        self.assertIn("gpt-6-luna; effort: high", question["criteria"][0])
        self.assertIn("gpt-6.1-sol; effort: medium", question["criteria"][1])
        response.read.assert_called_once_with(ROUTER.MAX_BYTES + 1)
        self.assertEqual(opener.open.call_args.kwargs["timeout"], ROUTER.TIMEOUT_SECONDS)

    def test_missing_auth_and_fixed_configuration_skip_network(self):
        with patch.object(ROUTER, "evaluate") as evaluate:
            self.assertEqual(
                ROUTER.select_configuration(PAYLOAD, None), {"status": "fallback", "reason": "missing_auth"}
            )
            fixed = copy.deepcopy(PAYLOAD)
            fixed["candidates"] = fixed["candidates"][:1]
            self.assertEqual(ROUTER.select_configuration(fixed, "test-placeholder")["reason"], "fixed_configuration")
            evaluate.assert_not_called()

    def test_invalid_inputs_do_not_send_task(self):
        cases = [
            {},
            {**PAYLOAD, "task": ""},
            {**PAYLOAD, "context": []},
            {**PAYLOAD, "candidates": []},
            {**PAYLOAD, "candidates": [None]},
            ladder_payload(ROUTER.MAX_CANDIDATES + 1),
        ]
        duplicate = copy.deepcopy(PAYLOAD)
        duplicate["candidates"][1]["id"] = "routine"
        cases.append(duplicate)
        for payload in cases:
            with self.subTest(payload=str(payload)[:60]), patch.object(ROUTER, "evaluate") as evaluate:
                self.assertEqual(ROUTER.select_configuration(payload, "test-placeholder")["reason"], "invalid_input")
                evaluate.assert_not_called()

    def test_malformed_or_oversized_wire_falls_back(self):
        cases = [
            b"{",
            b"\xff",
            b"[]",
            b'{"answers":{},"answers":{}}',
            b"x" * (ROUTER.MAX_BYTES + 1),
            {},
            {"answers": []},
            {"answers": {"configuration": {"type": "text", "text": "involved"}}},
            {
                "answers": {
                    "configuration": {
                        "type": "choice",
                        "choice": "involved",
                        "confidence": 0.8,
                        "probabilities": {"routine": 0.1, "involved": 0.9},
                    }
                }
            },
        ]
        for answer in cases:
            with self.subTest(answer=str(answer)[:40]):
                self.assertEqual(self.call(answer)[0]["reason"], "invalid_response")

    def test_distributions_are_complete_finite_and_unambiguous(self):
        cases = [
            ({"0": 0.2}, "invalid_response"),
            ({"0": 0.1, "1": 0.8, "2": 0.1}, "invalid_response"),
            ({"routine": 0.1, "involved": 0.9}, "invalid_response"),
            ({"0": -0.1, "1": 1.1}, "invalid_response"),
            ({"0": True, "1": 0}, "invalid_response"),
            ({"0": 0.1, "1": 0.8}, "invalid_response"),
            ({"0": float("nan"), "1": 0.8}, "invalid_response"),
            ({"0": float("inf"), "1": 0.8}, "invalid_response"),
            ({"0": 0.5, "1": 0.5}, "ambiguous_choice"),
        ]
        for probabilities, expected in cases:
            with self.subTest(probabilities=probabilities):
                answer = copy.deepcopy(ANSWER)
                answer["answers"]["configuration"].update(probabilities=probabilities, confidence=0)
                self.assertEqual(self.call(answer)[0]["reason"], expected)

    def test_confidence_numeric_types(self):
        for confidence in (-1, 1.1, True, "0.9", None, float("nan")):
            with self.subTest(confidence=confidence):
                answer = copy.deepcopy(ANSWER)
                answer["answers"]["configuration"]["confidence"] = confidence
                self.assertEqual(self.call(answer)[0]["reason"], "invalid_response")

    def test_confidence_must_match_score_distribution(self):
        for levels, confidence in (([0.49, 0.51], 1), ([0.1, 0.9], 0.78), ([0.72, 0.27, 0.01, 0], 0.6)):
            with self.subTest(levels=levels, confidence=confidence):
                result = self.call(ladder_answer(levels, confidence), payload=ladder_payload(len(levels)))[0]
                self.assertEqual(
                    result, {"status": "fallback", "reason": "invalid_response", "detail": "confidence_consistency"}
                )

    def test_live_score_distributions_match_formula_within_rounding(self):
        # Distributions and confidences observed from Jev, rounded independently to hundredths.
        cases = [
            ([0.72, 0.27, 0.01, 0], 0.71, "tier_0", "confidence"),
            ([0, 0.15, 0.83, 0.02], 0.83, "tier_2", "confidence"),
            ([0.64, 0.3, 0.05, 0.01, 0], 0.64, "tier_0", "confidence"),
            ([0, 0, 0.02, 0.1, 0.88], 0.88, "tier_4", "confidence"),
            ([0.03, 0.29, 0.65, 0.03], 0.62, "tier_2", "confidence"),
            ([0.91, 0.09], 0.82, "tier_0", "confidence"),
        ]
        for levels, confidence, expected_id, rule in cases:
            with self.subTest(levels=levels):
                result = self.call(ladder_answer(levels, confidence), payload=ladder_payload(len(levels)))[0]
                self.assertEqual(
                    (result["status"], result["id"], result["rule"], result["confidence"]),
                    ("selected", expected_id, rule, confidence),
                )

    def test_adjacent_tier_split_selects_higher_probability_tier(self):
        cases = [
            ([0.36, 0.47, 0.14, 0.03], "tier_1"),
            ([0.02, 0.1, 0.55, 0.33], "tier_2"),
            ([0.44, 0.4, 0.16, 0, 0], "tier_0"),
            ([0.45, 0.55], "tier_1"),
        ]
        for levels, expected_id in cases:
            with self.subTest(levels=levels):
                result = self.call(ladder_answer(levels), payload=ladder_payload(len(levels)))[0]
                self.assertEqual(result["status"], "selected")
                self.assertEqual(result["id"], expected_id)
                self.assertEqual(result["rule"], "adjacent_tiers")
                self.assertAlmostEqual(result["top_two_mass"], sorted(levels)[-1] + sorted(levels)[-2], places=3)
                self.assertLess(result["confidence"], ROUTER.CONFIDENCE_FLOOR)

    def test_distant_or_thin_splits_remain_abstentions(self):
        cases = [
            [0.36, 0.14, 0.35, 0.15],
            [0.45, 0.1, 0.44, 0.01],
            [0.5, 0.29, 0.21, 0],
            [0.45, 0.3, 0.25],
            [0.4, 0.3, 0.3],
            [0.42, 0.37, 0, 0.21],
        ]
        for levels in cases:
            with self.subTest(levels=levels):
                result = self.call(ladder_answer(levels), payload=ladder_payload(len(levels)))[0]
                self.assertEqual(result["status"], "fallback")
                self.assertEqual(result["reason"], "low_confidence")
                self.assertEqual(result["threshold"], ROUTER.CONFIDENCE_FLOOR)
                self.assertNotIn("rule", result)

    def test_rounding_tolerance_does_not_promote_confidence_rule(self):
        # Provider confidence meets the floor but the distribution recomputes below it.
        result = self.call(ladder_answer([0.17, 0.62, 0.17, 0.04], 0.6), payload=ladder_payload(4))[0]
        self.assertEqual(result["reason"], "low_confidence")
        self.assertEqual(result["confidence"], 0.6)
        self.assertAlmostEqual(result["recomputed_confidence"], 0.58, places=3)
        result = self.call(ladder_answer([0.8, 0.2], 0.59))[0]
        self.assertEqual(result["status"], "selected")
        self.assertEqual(result["rule"], "adjacent_tiers")

    def test_excessive_rounding_errors_return_fixed_details(self):
        for probabilities, confidence, detail in (
            ({"0": 0.11, "1": 0.91}, 0.82, "distribution_sum"),
            ({"0": 0.1, "1": 0.9}, 0.78, "confidence_consistency"),
            ({"0": 0.1}, 0.8, "distribution_keys"),
        ):
            with self.subTest(detail=detail):
                answer = copy.deepcopy(ANSWER)
                answer["answers"]["configuration"].update(probabilities=probabilities, confidence=confidence)
                self.assertEqual(
                    self.call(answer)[0],
                    {
                        "status": "fallback",
                        "reason": "invalid_response",
                        "detail": detail,
                    },
                )

    def test_validation_diagnostics_never_echo_response_or_credential(self):
        private = "malicious-response-and-test-placeholder"
        for answer, detail in (
            ({"answers": {"configuration": {"type": private}}}, "score_type"),
            ({"answers": {"configuration": {"type": "score", "probabilities": {private: 1}}}}, "distribution_keys"),
            (private.encode(), None),
        ):
            with self.subTest(detail=detail):
                result = self.call(answer)[0]
                self.assertEqual(result["reason"], "invalid_response")
                self.assertEqual(result.get("detail"), detail)
                self.assertNotIn(private, json.dumps(result))
                self.assertNotIn("test-placeholder", json.dumps(result))
        with patch.object(ROUTER, "evaluate", side_effect=ROUTER.InvalidData(private)):
            self.assertEqual(
                ROUTER.select_configuration(PAYLOAD, "test-placeholder"),
                {
                    "status": "fallback",
                    "reason": "invalid_response",
                },
            )

    def test_api_failure_timeout_and_redirects_use_safe_codes(self):
        for error, expected in (
            (urllib.error.URLError("private diagnostics"), "api_unavailable"),
            (
                urllib.error.HTTPError(ROUTER.ENDPOINT, 401, "private body", {}, io.BytesIO(b"secret")),
                "api_unavailable",
            ),
            (TimeoutError("private diagnostics"), "timeout"),
            (ROUTER.DeadlineExpired(), "timeout"),
            (urllib.error.URLError(TimeoutError()), "timeout"),
            (http.client.IncompleteRead(b"private partial body"), "api_unavailable"),
            (http.client.BadStatusLine("private status"), "api_unavailable"),
        ):
            with self.subTest(error=type(error).__name__):
                self.assertEqual(self.call(error=error)[0], {"status": "fallback", "reason": expected})
                if isinstance(error, urllib.error.HTTPError):
                    self.assertTrue(error.fp.closed)
        self.assertEqual(self.call(status=503)[0]["reason"], "api_unavailable")
        self.assertIsNone(ROUTER.NoRedirect().redirect_request(None, None, 302, "", {}, "https://example.com"))

    def test_deadline_bounds_entire_request_and_restores_handler(self):
        previous_handler = signal.getsignal(signal.SIGALRM)
        with patch.object(ROUTER.signal, "setitimer") as timer:
            with ROUTER.deadline():
                handler = signal.getsignal(signal.SIGALRM)
                with self.assertRaises(ROUTER.DeadlineExpired):
                    handler(signal.SIGALRM, None)
            self.assertEqual(signal.getsignal(signal.SIGALRM), previous_handler)
        self.assertEqual(timer.call_args_list[0].args, (signal.ITIMER_REAL, ROUTER.TIMEOUT_SECONDS))
        self.assertEqual(timer.call_args_list[-1].args, (signal.ITIMER_REAL, 0))

    def test_unavailable_deadline_uses_fallback(self):
        with patch.object(ROUTER, "deadline", side_effect=NotImplementedError):
            self.assertEqual(self.call()[0]["reason"], "request_unavailable")

    def test_cli_emits_only_safe_json_and_no_secret_or_input(self):
        environment = dict(os.environ)
        environment.pop("AI_GATEWAY_API_KEY", None)
        for raw, reason in (
            (json.dumps(PAYLOAD), "missing_auth"),
            ("{private-input", "invalid_input"),
            ("x" * (ROUTER.MAX_BYTES + 1), "invalid_input"),
        ):
            result = subprocess.run(
                [sys.executable, str(SCRIPT)], input=raw, text=True, capture_output=True, env=environment, timeout=2
            )
            self.assertEqual(result.returncode, 0)
            self.assertEqual(json.loads(result.stdout), {"status": "fallback", "reason": reason})
            self.assertEqual(result.stderr, "")


if __name__ == "__main__":
    unittest.main()
