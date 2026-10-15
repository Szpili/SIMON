"""HTTP contract tests with a fake agent and no external services."""

import http.client
import json
import threading
import unittest
from unittest.mock import patch

import simon_api


class GatewayTests(unittest.TestCase):
    def setUp(self):
        self.node = {"bootstrap": "unused", "model_hash": "fake-hash"}
        self.tool_calls = [
            {"id": "call_1", "type": "function",
             "function": {"name": "lookup", "arguments": '{"query":"SIMON"}'}},
            {"id": "call_2", "type": "function",
             "function": {"name": "status", "arguments": "{}"}},
        ]
        self.message = {"content": "I will check that.", "tool_calls": self.tool_calls,
                        "finish_reason": "tool_calls"}
        self.receipt = {"node_id": "node-1", "runtime": "fake",
                        "prompt_tokens": 7, "completion_tokens": 3,
                        "signature": "fake-signature"}
        self.result = {"ok": True, "receipt_zweryfikowany": True,
                       "output": json.dumps(self.message),
                       "receipt": self.receipt, "job_id": "j1"}
        for patcher in (
            patch.object(simon_api, "load_nodes", return_value={"test-model": [self.node]}),
            patch.object(simon_api, "_rr", {}),
            patch.object(simon_api, "JOBLOG", ""),
            patch.object(simon_api.Handler, "log_message"),
        ):
            patcher.start()
            self.addCleanup(patcher.stop)
        job_patch = patch.object(simon_api, "run_job", return_value=self.result)
        self.run_job = job_patch.start()
        self.addCleanup(job_patch.stop)
        self.server = simon_api.ThreadingHTTPServer(("127.0.0.1", 0), simon_api.Handler)
        self.addCleanup(self.server.server_close)
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()
        self.addCleanup(self.stop_server)

    def stop_server(self):
        self.server.shutdown()
        self.thread.join(timeout=5)
        self.assertFalse(self.thread.is_alive())

    def request(self, **overrides):
        request = {"model": "test-model", "messages": [{"role": "user", "content": "Hello"}]}
        request.update(overrides)
        connection = http.client.HTTPConnection(*self.server.server_address, timeout=5)
        self.addCleanup(connection.close)
        connection.request("POST", "/v1/chat/completions", json.dumps(request),
                           {"Content-Type": "application/json"})
        response = connection.getresponse()
        return response.status, response.headers, response.read().decode("utf-8")

    def test_non_stream_completion(self):
        status, headers, body = self.request()
        self.assertEqual(status, 200)
        data = json.loads(body)
        choice = data["choices"][0]
        self.assertEqual(choice["message"], {"role": "assistant",
                         "content": self.message["content"], "tool_calls": self.tool_calls})
        self.assertEqual(choice["finish_reason"], "tool_calls")
        self.assertEqual(data["simon"]["receipt"], self.receipt)
        self.assertEqual(headers["x-simon-job-id"], "j1")
        self.run_job.assert_called_once_with(self.node, {
            "model": "test-model", "messages": [{"role": "user", "content": "Hello"}]}, 512)

    def test_null_finish_reason_falls_back_to_tool_calls(self):
        self.message["finish_reason"] = None
        self.result["output"] = json.dumps(self.message)
        status, _, body = self.request()
        self.assertEqual(status, 200)
        self.assertEqual(json.loads(body)["choices"][0]["finish_reason"], "tool_calls")

    def test_stream_completion(self):
        status, headers, body = self.request(stream=True)
        self.assertEqual(status, 200)
        self.assertEqual(headers["Content-Type"], "text/event-stream")
        events = [event for event in body.split("\n\n") if event and not event.startswith(":")]
        self.assertEqual(events[-1], "data: [DONE]")
        self.assertTrue(all(event.startswith("data: ") for event in events))
        chunks = [json.loads(event[len("data: "):]) for event in events[:-1]]
        self.assertEqual(chunks[0]["choices"][0]["delta"],
                         {"role": "assistant", "content": ""})
        deltas = [chunk["choices"][0]["delta"] for chunk in chunks]
        self.assertIn({"content": self.message["content"]}, deltas)
        self.assertIn({"tool_calls": [dict(call, index=i)
                                     for i, call in enumerate(self.tool_calls)]}, deltas)
        self.assertEqual(chunks[-1]["choices"][0]["finish_reason"], "tool_calls")
        self.assertEqual(chunks[-1]["choices"][0]["delta"], {})
        self.assertEqual(chunks[-1]["simon"]["receipt"], self.receipt)

    def test_unverified_receipt_withholds_answer(self):
        self.result["receipt_zweryfikowany"] = False
        status, _, body = self.request()
        self.assertEqual(status, 502)
        self.assertEqual(json.loads(body)["error"]["type"], "simon_error")
        self.assertNotIn(self.message["content"], body)
        self.assertNotIn("tool_calls", body)
        self.assertNotIn("signed_output", body)

    def test_busy_node_returns_retry_after(self):
        self.run_job.side_effect = RuntimeError("node_zajety: queue full")
        status, headers, body = self.request()
        self.assertEqual(status, 503)
        self.assertEqual(headers["Retry-After"], "5")
        self.assertEqual(json.loads(body)["error"]["type"], "simon_busy")

    def test_empty_messages(self):
        status, _, body = self.request(messages=[])
        self.assertEqual(status, 400)
        self.assertEqual(json.loads(body)["error"]["message"], "empty messages")
        self.run_job.assert_not_called()

    def test_unknown_model(self):
        status, _, body = self.request(model="unknown")
        self.assertEqual(status, 404)
        self.assertIn("unknown model", json.loads(body)["error"]["message"])
        self.run_job.assert_not_called()


if __name__ == "__main__":
    unittest.main()
