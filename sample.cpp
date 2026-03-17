#include <iostream>

// Pure function: returns the square of x.
int square(int x) {
    return x * x;
}

// Pure function: returns the sum of a and b.
int add(int a, int b) {
    return a + b;
}

int main() {
    const int x = 7;
    const int y = 5;

    std::cout << "x = " << x << "\n";
    std::cout << "y = " << y << "\n";
    std::cout << "square(x) = " << square(x) << "\n";
    std::cout << "add(x, y) = " << add(x, y) << "\n";

    return 0;
}
