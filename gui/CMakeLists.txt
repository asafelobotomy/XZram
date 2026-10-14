cmake_minimum_required(VERSION 3.16)
project(xzram-qt VERSION 0.4.0 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_AUTOMOC ON)
set(CMAKE_AUTORCC ON)
set(CMAKE_AUTOUIC ON)

find_package(Qt6 REQUIRED COMPONENTS Widgets)

include(GNUInstallDirs)

# The GUI drives privileged operations through the xzram CLI, so it only ever runs the
# CLI from a fixed, build-time path (never $PATH or an environment override).
if(CMAKE_INSTALL_PREFIX_INITIALIZED_TO_DEFAULT)
    # Distribution packages build without a prefix but install the CLI to /usr/bin.
    set(_xzram_cli_default "/usr/bin/xzram")
else()
    set(_xzram_cli_default "${CMAKE_INSTALL_FULL_BINDIR}/xzram")
endif()
set(XZRAM_CLI_PATH "${_xzram_cli_default}" CACHE FILEPATH
    "Absolute path of the installed xzram CLI used by xzram-qt")
option(XZRAM_DEV_OVERRIDES
    "Development builds only: allow XZRAM_CLI (+ XZRAM_ALLOW_DEV_CLI=1) to override the CLI path"
    OFF)

add_subdirectory(xzram-qt)
