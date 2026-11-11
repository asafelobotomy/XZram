// The Memory and swap page, offscreen, with the stub org.uq.settings module and a fake ctx:
//   QT_QPA_PLATFORM=offscreen qmltestrunner -import tests/uq/stub -input tests/uq
import QtQuick
import QtTest

Item {
    id: root
    width: 700
    height: 700

    readonly property url pageUrl: Qt.resolvedUrl("../../uq/pages/xzram/Page.qml")

    readonly property var statusDoc: ({
            swaps: [],
            zram_devices: [{ name: "zram0", algorithm: "zstd", disk_size_bytes: 8589934592, data_bytes: 3221225472, compressed_bytes: 1073741824, total_bytes: 1181116006, mem_used_max_bytes: 1181116006, mount_point: "[SWAP]" }],
            memory: { mem_total_kb: 16777216, mem_available_kb: 8388608, swap_total_kb: 8388608, swap_free_kb: 7340032 }
        })
    readonly property var zramDoc: ({ device: "zram0", zram_size: "ram / 2", zram_resident_limit: null, compression_algorithm: "zstd", swap_priority: 100, fs_type: null, mount_point: null })
    readonly property var swapsDoc: [
        { name: "<b>/dev/zram0</b>‮", swap_type: "partition", size_bytes: 8589934592, used_bytes: 1073741824, priority: 100, source: "active", active: true }
    ]

    function reply(doc) {
        return { value: { json: JSON.stringify(doc) } };
    }

    function answers() {
        return {
            GetStatus: reply(statusDoc),
            GetZramConfig: reply(zramDoc),
            GetSysctl: reply({ swappiness: 60 }),
            ListSwaps: reply(swapsDoc),
            ApplyNowZram: { value: null },
            ApplyNowSysctl: { value: null }
        };
    }

    FakeContext {
        id: fake
        dbus: root.answers()
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

    function writes() {
        return fake.calls.filter(c => c[0] === "call" && c[1].indexOf("ApplyNow") === 0);
    }

    TestCase {
        name: "MemoryAndSwapPage"
        when: windowShown

        function load() {
            fake.calls = [];
            loader.source = "";
            loader.source = root.pageUrl;
            verify(loader.status === Loader.Ready, "the page loads");
            loader.item.ctx = fake.ctx;
        }

        function part(name) {
            const hit = findChild(loader.item, name);
            verify(hit, "found " + name);
            return hit;
        }

        function init() {
            fake.dbus = root.answers();
            fake.confirms = true;
            load();
            tryVerify(() => loader.item.phase === "ready");
        }

        function test_reads_on_open() {
            const methods = fake.calls.filter(c => c[0] === "call").map(c => c[1]).sort();
            compare(methods, ["GetStatus", "GetSysctl", "GetZramConfig", "ListSwaps"]);
            verify(findLabel(loader.item, "Memory: 8 GB in use of 16 GB") !== null);
            verify(findLabel(loader.item, "zram0: zstd, 8 GB, 3 GB stored in 1 GB (3 to 1)") !== null);
            compare(part("sizeBox").currentIndex, 1);
            compare(part("sizeBox").currentText, "Half of memory");
            compare(part("algorithmBox").currentText, "zstd");
            compare(part("swappinessSpin").value, 60);
            compare(root.writes().length, 0, "opening the page changes nothing");
        }

        function test_untrusted_text_is_shown_as_is() {
            verify(findLabel(loader.item, "<b>/dev/zram0</b>‮: partition, 1 GB used of 8 GB") !== null);
        }

        function test_nothing_is_written_before_confirm() {
            const box = part("sizeBox");
            box.currentIndex = 3;
            box.activated(3);
            compare(root.writes().length, 0);
            compare(part("confirmDialog").question, "Change the zram size? Swap in use is moved first; this can take a while.");
            part("confirmDialog").accept();
            tryVerify(() => root.writes().length === 1);
            const call = root.writes()[0];
            compare(call[1], "ApplyNowZram");
            const sent = JSON.parse(call[2][0]);
            compare(sent.zram_size, "ram");
            compare(sent.compression_algorithm, "zstd", "the other fields are kept");
            compare(sent.swap_priority, 100);
        }

        function test_algorithm_change_asks_first() {
            const box = part("algorithmBox");
            box.currentIndex = 1;
            box.activated(1);
            compare(root.writes().length, 0);
            part("confirmDialog").accept();
            tryVerify(() => root.writes().length === 1);
            compare(JSON.parse(root.writes()[0][2][0]).compression_algorithm, "lz4");
        }

        function test_cancel_is_silent() {
            const box = part("sizeBox");
            box.currentIndex = 3;
            box.activated(3);
            part("confirmDialog").reject();
            compare(root.writes().length, 0);
            compare(box.currentIndex, 1, "the control goes back to what was read");
            compare(part("problemLabel").visible, false);
            compare(part("noticeLabel").visible, false);
        }

        function test_failed_write_restores() {
            fake.dbus.ApplyNowZram = { error: "org.freedesktop.DBus.Error.AccessDenied: Not authorized" };
            const box = part("sizeBox");
            box.currentIndex = 3;
            box.activated(3);
            part("confirmDialog").accept();
            tryVerify(() => part("problemLabel").visible);
            compare(part("problemLabel").text, "Not changed. Administrator permission wasn't given.");
            compare(box.currentIndex, 1, "back to the last value read");
            // The raw error is behind Advanced details, not shown at first.
            compare(part("rawDetail").visible, false);
            part("advancedButton").clicked();
            verify(part("rawDetail").visible);
        }

        function test_swappiness_applies_on_apply_only() {
            const spin = part("swappinessSpin");
            spin.value = 90;
            compare(root.writes().length, 0, "stepping does not write");
            part("applySwappiness").clicked();
            tryVerify(() => root.writes().length === 1);
            compare(root.writes()[0][1], "ApplyNowSysctl");
            compare(root.writes()[0][2][0], "{\"swappiness\":90}");
            tryVerify(() => part("noticeLabel").visible);
            compare(part("noticeLabel").text, "Swappiness set to 90.");
        }

        function test_failed_swappiness_restores() {
            fake.dbus.ApplyNowSysctl = { error: "org.freedesktop.DBus.Error.AccessDenied: Not authorized" };
            const spin = part("swappinessSpin");
            spin.value = 90;
            part("applySwappiness").clicked();
            tryVerify(() => part("problemLabel").visible);
            compare(spin.value, 60);
        }

        function test_a_slow_write_is_not_a_failure() {
            fake.dbus.ApplyNowZram = { error: "no answer within 2 s" };
            const box = part("sizeBox");
            box.currentIndex = 0;
            box.activated(0);
            part("confirmDialog").accept();
            tryVerify(() => part("noticeLabel").visible);
            compare(part("noticeLabel").text, "Still working. Check again in a moment.");
            compare(part("problemLabel").visible, false);
        }

        function test_activation_error_shown() {
            fake.dbus = ({
                    GetStatus: { error: "org.freedesktop.DBus.Error.ServiceUnknown: The name io.github.XZram1 was not provided by any .service files" },
                    GetZramConfig: { error: "org.freedesktop.DBus.Error.ServiceUnknown" },
                    GetSysctl: { error: "org.freedesktop.DBus.Error.ServiceUnknown" },
                    ListSwaps: { error: "org.freedesktop.DBus.Error.ServiceUnknown" }
                });
            load();
            tryVerify(() => loader.item.phase === "unavailable");
            compare(part("unavailableLabel").text, "XZram isn't answering. It starts when needed, so try again in a moment.");
            // One retry of each read after the first answer.
            tryVerify(() => fake.calls.filter(c => c[0] === "call" && c[1] === "GetStatus").length === 2);
            // It recovers on Refresh.
            fake.dbus = root.answers();
            part("refreshButton").clicked();
            tryVerify(() => loader.item.phase === "ready");
        }

        function test_other_read_failure_shows_an_error_card() {
            fake.dbus.GetSysctl = { error: "something odd" };
            load();
            tryVerify(() => loader.item.phase === "error");
            verify(part("readError").visible);
            compare(part("applySwappiness").enabled, false);
        }

        function test_no_zram_config_disables_the_controls() {
            fake.dbus.GetZramConfig = root.reply(null);
            load();
            tryVerify(() => loader.item.phase === "ready" && part("notSetUp").visible);
            compare(part("sizeBox").enabled, false);
            compare(part("algorithmBox").enabled, false);
        }

        function test_more_in_xzram_opens_the_app() {
            part("moreButton").clicked();
            compare(fake.calls[fake.calls.length - 1], ["openApp", "io.github.XZram.desktop"]);
            tryVerify(() => part("moreButton").visible);
        }

        function test_highlight_scrolls_without_writing() {
            loader.item.highlight = "swaps";
            compare(loader.item.highlight, "swaps");
            compare(root.writes().length, 0);
        }
    }
}
