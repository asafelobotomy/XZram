// The Memory and swap page, offscreen, with the stub org.uq.settings module and a fake ctx:
//   QT_QPA_PLATFORM=offscreen qmltestrunner -import tests/uq/stub -input tests/uq
import QtQuick
import QtTest

Item {
    id: root
    width: 800
    height: 700

    readonly property url pageUrl: Qt.resolvedUrl("../../uq/pages/xzram/Page.qml")

    readonly property var statusDoc: ({
            swaps: [
                { name: "/dev/zram0", swap_type: "partition", size_bytes: 8589934592, used_bytes: 1073741824, priority: 100 },
                { name: "/swap/swapfile", swap_type: "file", size_bytes: 4294967296, used_bytes: 0, priority: 10 }
            ],
            zram_devices: [{ name: "<b>zram0</b>", algorithm: "zstd", disk_size_bytes: 8589934592, data_bytes: 3221225472, compressed_bytes: 1073741824, total_bytes: 1181116006, mem_used_max_bytes: 1181116006, mount_point: "[SWAP]" }],
            memory: { mem_total_kb: 16777216, mem_available_kb: 8388608, swap_total_kb: 8388608, swap_free_kb: 7340032 },
            pressure: { memory: { some_avg60: 0.0 } }
        })
    readonly property var configDoc: ({
            zram: { configured: true, device: "zram0", size: "ram / 2", algorithm: "zstd", priority: 100 },
            tuning: { swappiness: 60, page_cluster: 0, vfs_cache_pressure: 100, watermark_scale_factor: 125, watermark_boost_factor: 0 },
            pending: false, snapshots: 2
        })
    readonly property var snapshotsDoc: [
        { id: "20260116T101500-pre_apply", created_at: "2026-11-16T10:15:00Z", label: "Before change", trigger: "pre_apply" },
        { id: "20260115T090000-manual", created_at: "2026-11-15T09:00:00Z", label: "Taken by hand", trigger: "manual" }
    ]
    readonly property var planOk: ({ summary: "x", changes: [], would_refuse: false, reason: "" })
    readonly property var planLowMemory: ({ summary: "x", changes: [], would_refuse: true, reason: "not enough free memory: restarting zram0 brings 3.0 GiB back" })
    readonly property var doneEvents: [
        { event: "start", label: "Changing compressed memory" },
        { event: "progress", label: "Moving what is in swap back into memory", percent: 40, detail: "1.0 GiB left to move" },
        { event: "done", ok: true, percent: 100 }
    ]

    function answers() {
        const out = {
            "status": { code: 0, output: statusDoc },
            "config": { code: 0, output: configDoc },
            "doctor": { code: 3, error: "exit 3", output: { healthy: false, summary: "Healthy, 1 warning", issues: [{ severity: "warning", message: "No userspace OOM killer is active", suggestion: "Install systemd-oomd" }] } },
            "swapfiles": { code: 0, output: [{ path: "/swap/swapfile", size_mb: 4096, priority: 10 }] },
            "snapshots": { code: 0, output: snapshotsDoc },
            "snapshot-create": { code: 0, output: { id: "x" } }
        };
        for (const id of ["plan-zram", "plan-zram-boot", "plan-tuning", "plan-zram-off", "plan-zram-off-boot", "plan-swapfile-create", "plan-swapfile-resize", "plan-swapfile-remove", "plan-snapshot-restore", "plan-snapshot-delete", "plan-snapshot-prune", "plan-rollback"])
            out[id] = { code: 0, output: planOk };
        for (const id of ["set-zram", "set-zram-boot", "set-tuning", "zram-off", "zram-off-boot", "swapfile-create", "swapfile-resize", "swapfile-remove", "snapshot-restore", "snapshot-delete", "snapshot-prune", "rollback"])
            out[id] = { code: 0, output: null };
        return out;
    }

    function eventsFor() {
        const out = {};
        for (const id of ["set-zram", "set-zram-boot", "set-tuning", "zram-off", "zram-off-boot", "swapfile-create", "swapfile-resize", "swapfile-remove", "snapshot-restore", "snapshot-delete", "snapshot-prune", "rollback"])
            out[id] = doneEvents;
        return out;
    }

    FakeContext {
        id: fake
        commands: root.answers()
        events: root.eventsFor()
        confirmed: ["set-zram", "set-zram-boot", "set-tuning", "zram-off", "zram-off-boot", "swapfile-create", "swapfile-resize", "swapfile-remove", "snapshot-restore", "snapshot-delete", "snapshot-prune", "rollback"]
    }

    Loader {
        id: loader
        anchors.fill: parent
    }

    function findLabel(item, text) {
        if (item.text === text)
            return item;
        for (let i = 0; i < item.children.length; ++i) {
            const hit = findLabel(item.children[i], text);
            if (hit)
                return hit;
        }
        return null;
    }

    function find(item, name) {
        if (item.objectName === name)
            return item;
        for (let i = 0; i < item.children.length; ++i) {
            const hit = find(item.children[i], name);
            if (hit)
                return hit;
        }
        return null;
    }

    function runs(id) {
        return fake.calls.filter(c => c[1] === id);
    }

    TestCase {
        name: "MemoryAndSwapPage"
        when: windowShown

        function load(overrides, skipStatus) {
            fake.calls = [];
            fake.watches = ({});
            fake.confirms = true;
            const commands = root.answers();
            for (const k in (overrides || {}))
                commands[k] = overrides[k];
            fake.commands = commands;
            loader.source = "";
            loader.source = root.pageUrl;
            loader.item.ctx = fake.ctx;
            tryVerify(() => (skipStatus || loader.item.status !== null) && loader.item.config !== null && loader.item.snapshots !== null && loader.item.doctor !== null);
            return loader.item;
        }

        function noteOf(page, key) {
            tryVerify(() => page.noteFor(key) !== null);
            return page.noteFor(key);
        }

        function test_reads_everything_when_it_gets_its_context() {
            const page = load();
            verify(root.runs("status").length >= 1);
            verify(fake.calls.some(c => c[0] === "watch" && c[1] === "config"));
            verify(fake.calls.some(c => c[0] === "watch" && c[1] === "swapfiles"));
            compare(page.swapfiles.files.length, 1);
            compare(page.config.tuning["swappiness"], 60);
        }

        function test_overview_shows_memory_and_swap() {
            const page = load();
            verify(root.findLabel(page, "Memory: 8.0 GiB in use of 16.0 GiB") !== null);
            verify(root.findLabel(page, "Swap: 1.0 GiB in use of 8.0 GiB") !== null);
            verify(root.findLabel(page, "Memory pressure: None. Nothing is waiting for memory.") !== null);
            verify(root.findLabel(page, "3.0 GiB stored in 1.0 GiB (3 to 1)") !== null);
        }

        function test_text_from_xzram_is_shown_as_it_is() {
            const page = load();
            verify(root.findLabel(page, "<b>zram0</b>: zstd, 8.0 GiB") !== null);
        }

        function test_health_check_is_read_even_though_it_exits_3() {
            const page = load();
            verify(root.findLabel(page, "Healthy, 1 warning") !== null || page.doctor.summary === "Healthy, 1 warning");
            verify(root.findLabel(page, "Worth a look: No userspace OOM killer is active") !== null);
        }

        function test_a_failed_read_says_so() {
            const page = load({ "status": { error: "xzram: not installed" } }, true);
            tryVerify(() => page.statusProblem !== "");
            compare(page.statusProblem, "xzram: not installed");
        }

        function test_size_change_is_previewed_then_streamed() {
            const page = load();
            const box = root.find(page, "sizeBox");
            box.currentIndex = 0;
            box.activated(0);
            const note = noteOf(page, "zram-size");
            compare(note.message, "Compressed memory size changed.");
            compare(note.failed, false);
            compare(root.runs("plan-zram")[0], ["run", "plan-zram", ["zram-size", "ram / 4"]]);
            compare(root.runs("set-zram")[0], ["stream", "set-zram", ["zram-size", "ram / 4"]]);
            // The preview is asked for first.
            verify(fake.calls.indexOf(root.runs("plan-zram")[0]) < fake.calls.indexOf(root.runs("set-zram")[0]));
        }

        function test_cancelling_the_sheet_changes_and_says_nothing() {
            const page = load();
            fake.confirms = false;
            page.changeZram("zram-algorithm", "lz4");
            tryVerify(() => !page.busy && root.runs("set-zram").length === 1);
            wait(50);
            compare(page.noteFor("zram-algorithm"), null);
        }

        function test_not_enough_memory_offers_the_next_restart() {
            const page = load({ "plan-zram": { code: 0, output: root.planLowMemory } });
            page.changeZram("zram-size", "ram");
            const note = noteOf(page, "zram-size");
            compare(note.kind, "memory");
            compare(root.runs("set-zram").length, 0, "nothing was sent");
            verify(page.offer !== null, "the next restart is offered");
            compare(page.offer.command, "set-zram-boot");
            page.takeOffer();
            tryVerify(() => root.runs("set-zram-boot").length === 1);
            compare(root.runs("set-zram-boot")[0], ["stream", "set-zram-boot", ["zram-size", "ram"]]);
            compare(noteOf(page, "zram-size").message, "Saved. It takes effect at the next restart.");
        }

        function test_a_refusal_after_the_preview_is_worded() {
            const events = [{ event: "start", label: "x" }, { event: "error", ok: false, kind: "memory", error: "not enough free memory: …" }];
            fake.events = Object.assign({}, root.eventsFor(), { "set-zram": events });
            fake.commands = Object.assign({}, root.answers(), { "set-zram": { code: 1, error: "xzram: not enough free memory" } });
            const page = load({ "set-zram": { code: 1, error: "xzram: not enough free memory" } });
            fake.events = Object.assign({}, root.eventsFor(), { "set-zram": events });
            page.changeZram("zram-size", "ram");
            const note = noteOf(page, "zram-size");
            compare(note.failed, true);
            compare(note.kind, "memory");
            fake.events = root.eventsFor();
        }

        function test_a_bad_value_is_never_sent() {
            const page = load();
            page.changeTuning("swappiness", "999");
            compare(page.noteFor("swappiness").failed, true);
            compare(root.runs("set-tuning").length, 0);
            page.changeZram("zram-size", "-1");
            compare(root.runs("plan-zram").length, 0);
        }

        function test_swappiness_is_applied_from_its_box() {
            const page = load();
            const spin = root.find(page, "spin-swappiness");
            const apply = root.find(page, "apply-swappiness");
            verify(!apply.enabled, "nothing to apply while the box shows what is in force");
            spin.value = 100;
            tryVerify(() => apply.enabled);
            apply.clicked();
            tryVerify(() => root.runs("set-tuning").length === 1);
            compare(root.runs("set-tuning")[0], ["stream", "set-tuning", ["swappiness", "100"]]);
            compare(noteOf(page, "swappiness").message, "Swappiness set to 100.");
        }

        function test_nothing_set_up_offers_to_set_it_up() {
            const none = { zram: { configured: false, device: null, size: null, algorithm: null, priority: null }, tuning: root.configDoc.tuning, pending: false, snapshots: 0 };
            const page = load({ "config": { code: 0, output: none } });
            verify(root.find(page, "notSetUp") !== null);
            root.find(page, "setUpButton").clicked();
            tryVerify(() => root.runs("plan-zram").length === 1);
            compare(root.runs("plan-zram")[0][2], ["zram-algorithm", "zstd"]);
        }

        function test_turning_compressed_memory_off() {
            const page = load();
            root.find(page, "turnOffButton").clicked();
            tryVerify(() => root.runs("zram-off").length === 1);
            compare(noteOf(page, "zram-off").message, "Compressed memory is off.");
        }

        function test_a_swap_file_is_created_in_mib() {
            const page = load();
            root.find(page, "createButton").clicked();
            tryVerify(() => root.runs("swapfile-create").length === 1);
            compare(root.runs("plan-swapfile-create")[0][2], ["4096", "/swap/swapfile"]);
            compare(root.runs("swapfile-create")[0][2], ["4096", "/swap/swapfile"]);
        }

        function test_a_swap_file_is_removed() {
            const page = load();
            root.find(page, "remove-/swap/swapfile").clicked();
            tryVerify(() => root.runs("swapfile-remove").length === 1);
            compare(root.runs("swapfile-remove")[0][2], ["/swap/swapfile"]);
        }

        function test_a_swap_file_is_resized() {
            const page = load();
            root.find(page, "resize-/swap/swapfile").clicked();
            const field = root.find(page, "resizeField");
            compare(field.text, "4");
            field.text = "8";
            root.find(page, "resizeApply").clicked();
            tryVerify(() => root.runs("swapfile-resize").length === 1);
            compare(root.runs("swapfile-resize")[0][2], ["8192", "/swap/swapfile"]);
        }

        function test_a_snapshot_is_restored_after_its_preview() {
            const page = load();
            root.find(page, "restore-20260115T090000-manual").clicked();
            tryVerify(() => root.runs("snapshot-restore").length === 1);
            compare(root.runs("plan-snapshot-restore")[0][2], ["20260115T090000-manual"]);
        }

        function test_the_last_change_is_undone_with_a_rollback() {
            const page = load();
            const button = root.find(page, "rollbackButton");
            verify(button.enabled, "there is a snapshot from before a change");
            button.clicked();
            tryVerify(() => root.runs("rollback").length === 1);
        }

        function test_rollback_needs_a_snapshot_from_before_a_change() {
            const manual = [snapshotsOnly()[1]];
            const page = load({ "snapshots": { code: 0, output: manual } });
            verify(!root.find(page, "rollbackButton").enabled);
        }

        function snapshotsOnly() {
            return root.snapshotsDoc;
        }

        function test_old_snapshots_are_pruned() {
            const page = load();
            root.find(page, "pruneButton").clicked();
            tryVerify(() => root.runs("snapshot-prune").length === 1);
            compare(root.runs("snapshot-prune")[0][2], ["50"]);
        }

        function test_taking_a_snapshot_does_not_ask() {
            const page = load();
            root.find(page, "snapshotNow").clicked();
            compare(noteOf(page, "snapshot-new").message, "Snapshot taken.");
        }

        function test_only_one_change_runs_at_a_time() {
            const page = load();
            page.workingKey = "zram-size";
            page.changeTuning("swappiness", "90");
            compare(root.runs("set-tuning").length, 0);
            page.workingKey = "";
        }

        function test_a_change_re_reads_what_it_moved() {
            const page = load();
            const before = root.runs("status").length;
            page.changeTuning("page-cluster", "0");
            tryVerify(() => root.runs("status").length > before);
        }

        function test_a_followed_file_change_updates_the_page() {
            const page = load();
            const next = JSON.parse(JSON.stringify(root.configDoc));
            next.tuning.swappiness = 150;
            fake.commands = Object.assign({}, fake.commands, { "config": { code: 0, output: next } });
            fake.change("config");
            tryVerify(() => page.config.tuning["swappiness"] === 150);
        }
    }
}
