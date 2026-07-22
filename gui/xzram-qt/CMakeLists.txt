set(SOURCES
    main.cpp
    mainwindow.cpp
    xzramcli.cpp
    formatutils.cpp
    jsonloader.cpp
    resources.qrc
    widgets/pendingbanner.cpp
    widgets/dashboardwidget.cpp
    widgets/zramwidget.cpp
    widgets/swapfilewidget.cpp
    widgets/sysctlwidget.cpp
    widgets/doctorwidget.cpp
    widgets/recommendeddefaultsdialog.cpp
    widgets/snapshotwidget.cpp
    widgets/settingswidget.cpp
)

set(HEADERS
    mainwindow.h
    xzramcli.h
    formatutils.h
    jsonloader.h
    widgets/pendingbanner.h
    widgets/dashboardwidget.h
    widgets/zramwidget.h
    widgets/swapfilewidget.h
    widgets/sysctlwidget.h
    widgets/doctorwidget.h
    widgets/recommendeddefaultsdialog.h
    widgets/snapshotwidget.h
    widgets/settingswidget.h
)

add_executable(xzram-qt ${SOURCES} ${HEADERS})
target_link_libraries(xzram-qt PRIVATE Qt6::Widgets)
target_include_directories(xzram-qt PRIVATE ${CMAKE_CURRENT_SOURCE_DIR})
target_compile_definitions(xzram-qt PRIVATE
    XZRAM_QT_VERSION="${PROJECT_VERSION}"
)
install(TARGETS xzram-qt RUNTIME DESTINATION bin)
