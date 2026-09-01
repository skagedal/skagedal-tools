import 'package:flutter/material.dart';

import '../sitting_controller.dart';
import 'enso.dart';
import 'theme.dart';

/// The bell on its own, with no sitting attached.
///
/// Sometimes what is wanted is just the bell: to hear what a setting sounds
/// like, to mark the start of something that is not zazen, or to strike it
/// because it is a nice sound. There is no timer here and nothing is being
/// counted.
///
/// Two things to touch, in the two halves of the screen, because that is how
/// the bell itself works. The upper half is the bowl: tapping it strikes,
/// and strikes overlap the way they do on a real bell rather than cutting
/// each other off. The lower half is the striker laid down on the rim, which
/// stops the ring — the same gesture that closes a period of zazen.
class BellPage extends StatelessWidget {
  const BellPage({super.key, required this.controller});

  final SittingController controller;

  @override
  Widget build(BuildContext context) => Scaffold(
        appBar: AppBar(
          title: const Text(
            'BELL',
            style: TextStyle(letterSpacing: 6, fontSize: 14),
          ),
        ),
        body: SafeArea(
          child: ListenableBuilder(
            listenable: controller,
            builder: (context, _) => Column(
              children: [
                Expanded(
                  flex: 3,
                  child: _Strike(controller: controller),
                ),
                const Divider(),
                Expanded(
                  flex: 2,
                  child: _Damp(controller: controller),
                ),
              ],
            ),
          ),
        ),
      );
}

/// The bowl. Tap it to strike.
class _Strike extends StatelessWidget {
  const _Strike({required this.controller});

  final SittingController controller;

  @override
  Widget build(BuildContext context) => _Touchable(
        onTap: controller.strikeBell,
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 48, vertical: 16),
          child: Center(
            child: ConstrainedBox(
              constraints: const BoxConstraints(maxWidth: 280),
              // A full ring rather than a filling one: nothing is in
              // progress here, and an ensō that crept round would suggest
              // something was being measured.
              child: Enso(
                progress: 1,
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Text(
                      controller.settings.bell.label.toLowerCase(),
                      style: const TextStyle(
                        fontSize: 22,
                        fontWeight: FontWeight.w200,
                        letterSpacing: 3,
                      ),
                    ),
                    const SizedBox(height: 8),
                    const Text(
                      'strike',
                      style: TextStyle(
                        fontSize: 13,
                        letterSpacing: 2,
                        color: JikidoColors.faded,
                      ),
                    ),
                  ],
                ),
              ),
            ),
          ),
        ),
      );
}

/// The striker, laid on the rim. Tap to stop the ring.
class _Damp extends StatelessWidget {
  const _Damp({required this.controller});

  final SittingController controller;

  @override
  Widget build(BuildContext context) => _Touchable(
        onTap: controller.dampBell,
        child: const Center(
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              Icon(Icons.horizontal_rule, color: JikidoColors.faded, size: 40),
              SizedBox(height: 4),
              Text(
                'rest the striker',
                style: TextStyle(
                  fontSize: 13,
                  letterSpacing: 2,
                  color: JikidoColors.faded,
                ),
              ),
            ],
          ),
        ),
      );
}

/// A whole half of the screen that answers a tap.
///
/// The target being this big is the point: this is a screen to use with your
/// eyes somewhere else, and hunting for a button defeats it.
class _Touchable extends StatelessWidget {
  const _Touchable({required this.onTap, required this.child});

  final VoidCallback onTap;
  final Widget child;

  @override
  Widget build(BuildContext context) => GestureDetector(
        onTap: onTap,
        behavior: HitTestBehavior.opaque,
        child: SizedBox.expand(child: child),
      );
}
