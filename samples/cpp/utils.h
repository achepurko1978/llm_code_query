#pragma once

#include <string>
#include <vector>

namespace utils {

/// Clamp a value between lo and hi.
template<typename T>
T clamp(T value, T lo, T hi) {
    if (value < lo) return lo;
    if (value > hi) return hi;
    return value;
}

/// Simple key-value pair.
struct KeyValue {
    std::string key;
    std::string value;
};

/// Split a string by a delimiter character.
std::vector<std::string> split(const std::string& s, char delim);

/// Join strings with a separator.
std::string join(const std::vector<std::string>& parts, const std::string& sep);

/// Count occurrences of a character in a string.
int count_char(const std::string& s, char c);

} // namespace utils
