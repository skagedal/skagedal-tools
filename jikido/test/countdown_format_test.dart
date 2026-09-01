import 'package:flutter_test/flutter_test.dart';
import 'package:jikido/src/ui/sitting_page.dart';

void main() {
  test('formats a countdown as minutes and seconds', () {
    expect(formatRemaining(const Duration(minutes: 15)), '15:00');
    expect(formatRemaining(const Duration(minutes: 4, seconds: 7)), '4:07');
    expect(formatRemaining(Duration.zero), '0:00');
  });

  test('grows an hours field only when there are hours', () {
    expect(formatRemaining(const Duration(minutes: 59, seconds: 59)), '59:59');
    expect(formatRemaining(const Duration(hours: 1)), '1:00:00');
    expect(
      formatRemaining(const Duration(hours: 1, minutes: 5, seconds: 3)),
      '1:05:03',
    );
  });
}
