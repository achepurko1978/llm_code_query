namespace model {

class BaseCounter {
public:
    virtual int bump(int value) {
        return value + 1;
    }
};

class FancyCounter : public BaseCounter {
public:
    int bump(int value) override {
        return BaseCounter::bump(value) + 1;
    }

    int twice(int value) {
        return bump(value) * 2;
    }
};

int run_counter(int seed) {
    FancyCounter c;
    return c.twice(seed);
}

}  // namespace model
