cmake_minimum_required(VERSION 3.16)

project(fun_cpp_sample LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)

add_executable(fun_sample sample.cpp)

add_library(sample_functions OBJECT samples/cpp/functions.cpp)
add_library(sample_classes OBJECT samples/cpp/classes.cpp)
add_library(sample_data OBJECT samples/cpp/data.cpp)
