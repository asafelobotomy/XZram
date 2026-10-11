set(SOURCES
    main.cpp
    mainwindow.cpp
    mainwindow_actions.cpp
    mainwindow_refresh.cpp
    xzramcli.cpp
    xzramcli_args.cpp
    clijob.cpp
    privilegedjob.cpp
    refreshrunner.cpp
    formatutils.cpp
    jsonloader.cpp
    linkedoptimize.cpp
    resources.qrc
    widgets/pendingbanner.cpp
    widgets/dashboardwidget.cpp
    widgets/zramwidget.cpp
    widgets/swapfilewidget.cpp
    widgets/swapfilewidget_btrfs.cpp
    widgets/swapfilewidget_linked.cpp
    widgets/swapfilewidget_partition.cpp
    widgets/sysctlwidget.cpp
    widgets/doctorwidget.cpp
    widgets/recommendeddefaultsdialog.cpp
    widgets/recommendeddefaultsui.cpp
    widgets/recommendeddefaultsscale.cpp
    widgets/snapshotwidget.cpp
    widgets/settingswidget.cpp
)

set(HEADERS
    mainwindow.h
    xzramcli.h
    clijob.h
    privilegedjob.h
    refreshrunner.h
    formatutils.h
    jsonloader.h
    linkedoptimize.h
    widgets/pendingbanner.h
    widgets/dashboardwidget.h
    widgets/zramwidget.h
    widgets/swapfilewidget.h
    widgets/sysctlwidget.h
    widgets/doctorwidget.h
    widgets/recommendeddefaultsdialog.h
    widgets/recommendeddefaultsui.h
    widgets/recommendeddefaultsscale.h
    widgets/snapshotwidget.h
    widgets/settingswidget.h
)

add_executable(xzram-qt ${SOURCES} ${HEADERS})
target_link_libraries(xzram-qt PRIVATE Qt6::Widgets)
target_include_directories(xzram-qt PRIVATE ${CMAKE_CURRENT_SOURCE_DIR})
target_compile_definitions(xzram-qt PRIVATE
    XZRAM_QT_VERSION="${PROJECT_VERSION}"
    XZRAM_CLI_INSTALL_PATH="${XZRAM_CLI_PATH}"
)
if(XZRAM_DEV_OVERRIDES)
    # Development only: honour XZRAM_CLI when XZRAM_ALLOW_DEV_CLI is set.
    target_compile_definitions(xzram-qt PRIVATE XZRAM_DEV_OVERRIDES)
endif()
install(TARGETS xzram-qt RUNTIME DESTINATION bin)
# Theme icon for desktop/taskbar (matches Icon=io.github.XZram in the .desktop file).
install(FILES icons/xzram-icon.png
    DESTINATION share/icons/hicolor/512x512/apps
    RENAME io.github.XZram.png)
