import 'dart:math' as math;

import 'package:flutter/material.dart';

import 'theme.dart';

/// A circle drawn the way an ensō is drawn: in one stroke, opening at the
/// top, thicker where the brush presses and thinner where it lifts, and
/// never quite closed.
///
/// The filled part of the stroke is how much of the sitting has passed.
class Enso extends StatelessWidget {
  const Enso({super.key, required this.progress, required this.child});

  /// 0.0 at the opening bell, 1.0 at the closing one.
  final double progress;

  final Widget child;

  @override
  Widget build(BuildContext context) => AspectRatio(
        aspectRatio: 1,
        child: CustomPaint(
          painter: _EnsoPainter(progress: progress.clamp(0.0, 1.0)),
          child: Center(child: child),
        ),
      );
}

class _EnsoPainter extends CustomPainter {
  const _EnsoPainter({required this.progress});

  final double progress;

  /// The brush starts a little past the top and comes back round to just
  /// short of where it began, leaving the gap that makes it an ensō.
  static const double _startAngle = -math.pi / 2 + 0.16;
  static const double _sweepAngle = 2 * math.pi - 0.32;

  /// The stroke is drawn as this many short arcs, each with its own width.
  /// Enough segments that the taper reads as continuous.
  static const int _segments = 96;

  @override
  void paint(Canvas canvas, Size size) {
    final baseWidth = size.shortestSide * 0.035;
    final radius = size.shortestSide / 2 - baseWidth * 1.6;
    final rect = Rect.fromCircle(
      center: Offset(size.width / 2, size.height / 2),
      radius: radius,
    );

    // The unbrushed circle is drawn as one thin arc rather than as segments:
    // it is a guide, and at this opacity the overlaps between segments would
    // show up as a scalloped edge.
    canvas.drawArc(
      rect,
      _startAngle,
      _sweepAngle,
      false,
      Paint()
        ..color = const Color(0x1FE9E2D5)
        ..style = PaintingStyle.stroke
        ..strokeCap = StrokeCap.round
        ..strokeWidth = baseWidth * 0.5,
    );

    if (progress > 0) {
      _strokeBrush(canvas, rect, baseWidth, progress);
    }
  }

  void _strokeBrush(
    Canvas canvas,
    Rect rect,
    double baseWidth,
    double fraction,
  ) {
    final paint = Paint()
      ..color = JikidoColors.vermilion
      ..style = PaintingStyle.stroke
      ..strokeCap = StrokeCap.round;

    final segments = math.max(1, (_segments * fraction).round());
    final segmentSweep = _sweepAngle * fraction / segments;

    for (var i = 0; i < segments; i++) {
      // `t` runs 0..1 along the *whole* circle, not along the drawn part, so
      // that a half-finished sitting looks like a half-finished brushstroke
      // rather than a complete small one.
      final t = (i + 0.5) / _segments;
      paint.strokeWidth = baseWidth * _brushWidth(t);
      canvas.drawArc(
        rect,
        _startAngle + segmentSweep * i,
        // A hair of overlap, so the seams between segments do not show.
        segmentSweep * 1.35,
        false,
        paint,
      );
    }
  }

  /// How hard the brush is pressing, as a fraction of the nominal width.
  /// Light on contact, heaviest around two thirds of the way round, lifting
  /// off at the end.
  static double _brushWidth(double t) {
    final swell = 0.45 + 0.9 * math.sin(math.pi * math.pow(t, 0.8).toDouble());
    final liftOff = t > 0.82 ? 1 - (t - 0.82) / 0.18 * 0.7 : 1.0;
    final touchDown = t < 0.08 ? 0.3 + t / 0.08 * 0.7 : 1.0;
    return swell * liftOff * touchDown;
  }

  @override
  bool shouldRepaint(_EnsoPainter oldDelegate) =>
      oldDelegate.progress != progress;
}
