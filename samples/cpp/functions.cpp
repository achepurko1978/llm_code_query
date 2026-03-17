namespace fun {

int add(int a, int b) {
    return a + b;
}

int add(int a, int b, int c) {
    return a + b + c;
}

int square(int x) {
    return x * x;
}

int combined(int x, int y) {
    return square(add(x, y));
}

}  // namespace fun
