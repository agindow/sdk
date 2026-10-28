import unittest
from typing import Callable
from agindow import Agindow, Config, RenameRule, SelectiveRepair, AgindowDriftError, AgindowSetupError, RepairCheck, Mode


def verify(check: RepairCheck) -> bool:
    import json
    return bool(json.loads(check.before)['old_name'] == json.loads(check.after)['display_name'])


class SelectiveRepairTests(unittest.TestCase):
    rule = RenameRule(tool='contacts.create', side='request', source='old_name', target='display_name', migration='reviewed-2026-10', confidence=1.0)

    def sdk(self, mode: Mode = 'repair', verifier: Callable[[RepairCheck], bool] = verify, rule: RenameRule | None = None) -> Agindow:
        return Agindow(Config(mode=mode), selective_repair=SelectiveRepair((rule or self.rule,), verifier))

    def test_default_observe_and_opt_in_request(self) -> None:
        value = {'old_name': 'Ada', 'untouched': {'id': 7}}
        for sdk in [Agindow(Config(mode='repair')), self.sdk('observe')]:
            self.assertIs(sdk.guard(lambda x: x, tool=self.rule.tool)(value), value)
        sdk = self.sdk()
        self.assertTrue(sdk.ready, str(sdk.load_error))
        self.assertEqual(sdk.guard(lambda x: x, tool=self.rule.tool, operation='write')(value), {'display_name': 'Ada', 'untouched': {'id': 7}})
        self.assertEqual(value['old_name'], 'Ada')
        self.assertEqual(sum(e.type == 'repair' for e in sdk.recent()), 1)

    def test_scope_refusal_collision_and_fault(self) -> None:
        value = {'old_name': 'Ada'}
        sdk = self.sdk()
        self.assertIs(sdk.guard(lambda x: x, tool='other')(value), value)
        collision = {**value, 'display_name': 'Other'}
        self.assertIs(sdk.guard(lambda x: x, tool=self.rule.tool)(collision), collision)
        refused = self.sdk(verifier=lambda _: False)
        self.assertIs(refused.guard(lambda x: x, tool=self.rule.tool)(value), value)
        strict = self.sdk('strict', lambda _: False)
        with self.assertRaises(AgindowDriftError):
            strict.guard(lambda _: self.fail('must not dispatch'), tool=self.rule.tool)(value)
        def fault(_: RepairCheck) -> bool:
            raise RuntimeError('verification unavailable')
        broken = self.sdk(verifier=fault)
        self.assertIs(broken.guard(lambda x: x, tool=self.rule.tool)(value), value)
        self.assertEqual(sum(e.type == 'bypass' for e in broken.recent()), 1)

    def test_async_response(self) -> None:
        import asyncio
        from dataclasses import replace
        sdk = self.sdk('strict', rule=replace(self.rule, side='response'))
        @sdk.guard(tool=self.rule.tool)
        async def read() -> dict[str, object]:
            return {'old_name': 'Grace', 'id': 1}
        self.assertEqual(asyncio.run(read()), {'display_name': 'Grace', 'id': 1})

    def test_invalid_configuration(self) -> None:
        with self.assertRaises(AgindowSetupError):
            Agindow(selective_repair=SelectiveRepair((self.rule, self.rule), verify))

    def test_statistical_veto_and_mode_toggle(self) -> None:
        import json
        calls: list[RepairCheck] = []
        def checked(check: RepairCheck) -> bool:
            calls.append(check)
            return True
        selected = Agindow(Config(mode='repair'), selective_repair=SelectiveRepair((self.rule,), checked),
                           selection={'tenant': 't', 'connection': 'c', 'vendor': 'crm', 'operation': self.rule.tool,
                                      'segment': 's', 'fields': [{'side': 'request', 'path': '/amount', 'enumeration': True,
                                       'baseline': [json.dumps({'amount': 1})] * 128, 'live': [json.dumps({'amount': 100})] * 128}]})
        with self.assertRaises(AgindowDriftError):
            selected.guard(lambda _: self.fail('must not dispatch'), tool=self.rule.tool, operation='write')({'old_name': 'Ada', 'amount': 100})
        self.assertEqual(calls, [])
        sdk = self.sdk('observe')
        fn = sdk.guard(lambda x: x, tool=self.rule.tool)
        value = {'old_name': 'Ada'}
        self.assertIs(fn(value), value)
        sdk.configure(mode='repair')
        self.assertEqual(fn(value), {'display_name': 'Ada'})
        sdk.configure(mode='observe')
        self.assertIs(fn(value), value)

    def test_unverified_write_policy_and_upstream_exception(self) -> None:
        value = {'old_name': 'Ada'}
        sdk = self.sdk(verifier=lambda _: False)
        self.assertIs(sdk.guard(lambda x: x, tool=self.rule.tool, operation='write')(value), value)
        sdk.configure(fail_closed=False)
        self.assertIs(sdk.guard(lambda x: x, tool=self.rule.tool, operation='write')(value), value)
        approved = self.sdk()
        error = RuntimeError('vendor error')
        def fail(_: dict[str, str]) -> None:
            raise error
        with self.assertRaises(RuntimeError) as caught:
            approved.guard(fail, tool=self.rule.tool)(value)
        self.assertIs(caught.exception, error)
