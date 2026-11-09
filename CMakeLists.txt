cmake_minimum_required(VERSION 3.20)
project(CellHub LANGUAGES CXX)
if(NOT EMSCRIPTEN)
    message(FATAL_ERROR "Use: emcmake cmake -S . -B build")
endif()
add_executable(cellhub src/main.cpp)
target_compile_features(cellhub PRIVATE cxx_std_17)
target_compile_options(cellhub PRIVATE -Wall -Wextra -Wpedantic)
set_target_properties(cellhub PROPERTIES OUTPUT_NAME index SUFFIX .html)
target_link_options(cellhub PRIVATE
    "--shell-file=${CMAKE_CURRENT_SOURCE_DIR}/web/shell.html"
    -sMIN_WEBGL_VERSION=2 -sMAX_WEBGL_VERSION=2
    -sALLOW_MEMORY_GROWTH=1 -sENVIRONMENT=web)
set_property(TARGET cellhub APPEND PROPERTY LINK_DEPENDS "${CMAKE_CURRENT_SOURCE_DIR}/web/shell.html")
