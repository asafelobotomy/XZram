set(SOURCES
    main.cpp
    mainwindow.cpp
    dbusclient.cpp
    clifallback.cpp
    formatutils.cpp
    jsonloader.cpp
    widgets/pendingbanner.cpp
    widgets/dashboardwidget.cpp
    widgets/zramwidget.cpp
    widgets/swapfilewidget.cpp
    widgets/sysctlwidget.cpp
    widgets/doctorwidget.cpp
    widgets/recommendeddefaultsdialog.cpp
    widgets/utilitieswidget.cpp
)

set(HEADERS
    mainwindow.h
    dbusclient.h
    clifallback.h
    formatutils.h
    jsonloader.h
    widgets/pendingbanner.h
    widgets/dashboardwidget.h
    widgets/zramwidget.h
    widgets/swapfilewidget.h
    widgets/sysctlwidget.h
    widgets/doctorwidget.h
    widgets/recommendeddefaultsdialog.h
    widgets/utilitieswidget.h
)

add_executable(xzram-qt ${SOURCES} ${HEADERS})
target_link_libraries(xzram-qt PRIVATE Qt6::Widgets Qt6::DBus)
target_include_directories(xzram-qt PRIVATE ${CMAKE_CURRENT_SOURCE_DIR})
install(TARGETS xzram-qt RUNTIME DESTINATION bin)
