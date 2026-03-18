#pragma once

namespace geometry {

/// Abstract base class for 2D shapes.
class Shape {
public:
    virtual ~Shape() = default;
    virtual double area() const = 0;
    virtual double perimeter() const = 0;
};

class Circle : public Shape {
    double radius_;
public:
    explicit Circle(double r) : radius_(r) {}
    double area() const override;
    double perimeter() const override;
    double radius() const { return radius_; }
};

class Rectangle : public Shape {
    double w_, h_;
public:
    Rectangle(double w, double h) : w_(w), h_(h) {}
    double area() const override;
    double perimeter() const override;
    double width() const { return w_; }
    double height() const { return h_; }
};

/// Free function: compute the total area of an array of shapes.
double total_area(const Shape* shapes[], int count);

} // namespace geometry
