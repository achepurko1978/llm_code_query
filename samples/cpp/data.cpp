namespace data {

struct Point {
    int x;
    int y;

    int manhattan() const {
        return x + y;
    }
};

Point make_point(int x, int y) {
    return Point{x, y};
}

int magnitude_hint(const Point& p) {
    return p.manhattan();
}

}  // namespace data
