class CommandBufferMaker {
  constructor(t, encoder, finish) {
    this.encoder = encoder;
    this.finish = finish;
    this.validateFinish = (shouldSucceed) => {
      return t.expectGPUError('validation', this.finish, !shouldSucceed);
    };
    this.validateFinishAndSubmit = (shouldBeValid, submitShouldSucceedIfValid) => {
      const commandBuffer = this.validateFinish(shouldBeValid);
      if (shouldBeValid) {
        t.expectValidationError(() => t.queue.submit([commandBuffer]), !submitShouldSucceedIfValid);
      }
    };
  }
}

async function body(t) {
  for (const encoderType of ['render bundle', 'render pass']) {
    for (const flag of [false, true]) {
      new CommandBufferMaker(t, encoderType, () => encoderType);
    }
  }
}

body({ expectGPUError() {}, expectValidationError() {}, queue: { submit() {} } });
